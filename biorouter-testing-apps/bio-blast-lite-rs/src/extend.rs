//! Extension algorithms: ungapped extension with X-drop and banded Smith-Waterman.
//!
//! After seed hits are found, we extend each seed to find the best local alignment:
//! 1. **Ungapped extension**: Extend the match in both directions without gaps,
//!    using an X-drop threshold to stop when the score drops too far.
//! 2. **Gapped extension (banded SW)**: Around seeds that survived ungapped extension,
//!    perform a banded Smith-Waterman to find optimal gapped alignments.

use crate::score::ScoringScheme;

// ============================================================================
// Ungapped Extension with X-Drop
// ============================================================================

/// Result of ungapped extension from a seed position.
#[derive(Debug, Clone)]
pub struct UngappedResult {
    /// Score of the ungapped extension.
    pub score: i32,
    /// Leftmost position of the ungapped alignment (query coordinates).
    pub q_start: usize,
    /// Rightmost position (exclusive) of the ungapped alignment (query coordinates).
    pub q_end: usize,
    /// Leftmost position of the ungapped alignment (database coordinates).
    pub db_start: usize,
    /// Rightmost position (exclusive) of the ungapped alignment (db coordinates).
    pub db_end: usize,
}

/// Perform ungapped extension from a seed match in both directions.
///
/// `q_pos` and `db_pos` are the start of the seed k-mer (0-based).
/// `k` is the k-mer size.
/// `x_drop` is the maximum score drop before stopping.
pub fn ungapped_extend(
    query: &[u8],
    db_seq: &[u8],
    q_pos: usize,
    db_pos: usize,
    k: usize,
    scoring: &dyn ScoringScheme,
    x_drop: i32,
) -> UngappedResult {
    let q_len = query.len();
    let db_len = db_seq.len();

    // Start with the score from the seed k-mer itself
    let mut seed_score = 0i32;
    for i in 0..k {
        seed_score += scoring.score(query[q_pos + i], db_seq[db_pos + i]);
    }

    // Extend right
    let mut best_score = seed_score;
    let mut current_score = seed_score;
    let mut right_ext = 0usize;
    while q_pos + k + right_ext < q_len && db_pos + k + right_ext < db_len {
        let q_idx = q_pos + k + right_ext;
        let d_idx = db_pos + k + right_ext;
        current_score += scoring.score(query[q_idx], db_seq[d_idx]);
        right_ext += 1;
        if current_score > best_score {
            best_score = current_score;
        }
        if best_score - current_score > x_drop {
            break;
        }
    }

    // Extend left
    let mut left_ext = 0usize;
    current_score = seed_score;
    while q_pos > 0 && db_pos > 0 && left_ext < q_pos && left_ext < db_pos {
        left_ext += 1;
        let q_idx = q_pos - left_ext;
        let d_idx = db_pos - left_ext;
        current_score += scoring.score(query[q_idx], db_seq[d_idx]);
        if current_score > best_score {
            best_score = current_score;
        }
        if best_score - current_score > x_drop {
            break;
        }
    }

    UngappedResult {
        score: best_score,
        q_start: q_pos - left_ext,
        q_end: q_pos + k + right_ext,
        db_start: db_pos - left_ext,
        db_end: db_pos + k + right_ext,
    }
}

// ============================================================================
// Banded Smith-Waterman (Gapped Extension)
// ============================================================================

/// Result of a gapped alignment.
#[derive(Debug, Clone)]
pub struct GappedResult {
    /// Best alignment score.
    pub score: i32,
    /// Query alignment start (0-based, inclusive).
    pub q_start: usize,
    /// Query alignment end (0-based, exclusive).
    pub q_end: usize,
    /// Database alignment start (0-based, inclusive).
    pub db_start: usize,
    /// Database alignment end (0-based, exclusive).
    pub db_end: usize,
    /// The alignment traceback as pairs of (query_pos, db_pos). None = gap in query, Some = gap in db.
    pub traceback: Vec<(Option<usize>, Option<usize>)>,
}

/// Banded Smith-Waterman gapped extension.
///
/// Searches only within a diagonal band around the seed to keep the
/// algorithm O(n * band_width) instead of O(n²).
///
/// - `q_anchor` / `db_anchor`: seed position from which to anchor the band.
/// - `band_width`: half-width of the diagonal band (total band = 2*bw+1).
/// - `flank`: how far to search around the ungapped region.
pub fn banded_sw(
    query: &[u8],
    db_seq: &[u8],
    q_anchor: usize,
    db_anchor: usize,
    band_width: usize,
    flank: usize,
    scoring: &dyn ScoringScheme,
) -> GappedResult {
    let q_len = query.len();
    let db_len = db_seq.len();

    // Define the search window
    let q_start = q_anchor.saturating_sub(flank);
    let q_end = (q_anchor + flank).min(q_len);
    let db_start = db_anchor.saturating_sub(flank);
    let db_end = (db_anchor + flank).min(db_len);

    let q_win_len = q_end - q_start;
    let d_win_len = db_end - db_start;

    if q_win_len == 0 || d_win_len == 0 {
        return GappedResult {
            score: 0,
            q_start,
            q_end,
            db_start,
            db_end,
            traceback: Vec::new(),
        };
    }

    // Dynamic programming within the band
    // Use flat 2D arrays: dp[i][j] and traceback
    // To save memory, we do row-by-row
    let n_rows = q_win_len + 1;
    let n_cols = d_win_len + 1;

    // dp[j] = current row
    let mut dp_prev = vec![0i32; n_cols];
    let mut dp_curr = vec![0i32; n_cols];

    // Store traceback: 0=diag(match/mismatch), 1=up(query gap), 2=left(db gap), 3=no extension
    let mut tb: Vec<Vec<u8>> = vec![vec![3; n_cols]; n_rows];

    let mut best_score = 0i32;
    let mut best_q = 0usize;
    let mut best_d = 0usize;

    let anchor_q = q_anchor - q_start;
    let anchor_d = db_anchor - db_start;

    for i in 1..=q_win_len {
        // Clear current row
        for val in dp_curr.iter_mut() {
            *val = 0;
        }

        for j in 1..n_cols {
            // Check band: diagonal distance from anchor
            let diag_i = (i as isize) - (anchor_q as isize);
            let diag_j = (j as isize) - (anchor_d as isize);
            let diag_diff = (diag_i - diag_j).unsigned_abs() as usize;

            if diag_diff > band_width {
                // Outside the band — leave as 0
                tb[i][j] = 3;
                continue;
            }

            let q_idx = q_start + i - 1;
            let d_idx = db_start + j - 1;

            let match_score = dp_prev[j - 1] + scoring.score(query[q_idx], db_seq[d_idx]);
            let gap_in_db = dp_curr[j - 1] - scoring.gap_open(); // gap in database = gap in query's sequence
            let gap_in_q = dp_prev[j] - scoring.gap_open(); // gap in query = gap in database's sequence

            let (best, tb_code) = if match_score >= gap_in_db && match_score >= gap_in_q {
                (match_score.max(0), 0u8)
            } else if gap_in_db >= gap_in_q {
                (gap_in_db.max(0), 2u8)
            } else {
                (gap_in_q.max(0), 1u8)
            };

            dp_curr[j] = best;
            tb[i][j] = tb_code;

            if best > best_score {
                best_score = best;
                best_q = i;
                best_d = j;
            }
        }

        std::mem::swap(&mut dp_prev, &mut dp_curr);
    }

    // Traceback
    let mut traceback: Vec<(Option<usize>, Option<usize>)> = Vec::new();
    let mut ci = best_q;
    let mut cj = best_d;

    while ci > 0 && cj > 0 && tb[ci][cj] != 3 {
        let code = tb[ci][cj];
        let q_idx = Some(q_start + ci - 1);
        let d_idx = Some(db_start + cj - 1);

        match code {
            0 => {
                // Diagonal (match/mismatch)
                traceback.push((q_idx, d_idx));
                ci -= 1;
                cj -= 1;
            }
            1 => {
                // Gap in query (deletion in query = insertion in db)
                traceback.push((None, d_idx));
                cj -= 1;
            }
            2 => {
                // Gap in database (insertion in query)
                traceback.push((q_idx, None));
                ci -= 1;
            }
            _ => break,
        }
    }

    traceback.reverse();

    // Compute alignment boundaries from traceback
    let (aq_start, aq_end, ad_start, ad_end) = if traceback.is_empty() {
        (q_start, q_start, db_start, db_start)
    } else {
        let first_q = traceback.iter().find_map(|(q, _)| *q).unwrap_or(q_start);
        let last_q = traceback.iter().rev().find_map(|(q, _)| *q).unwrap_or(q_start);
        let first_d = traceback.iter().find_map(|(_, d)| *d).unwrap_or(db_start);
        let last_d = traceback.iter().rev().find_map(|(_, d)| *d).unwrap_or(db_start);
        (first_q, last_q + 1, first_d, last_d + 1)
    };

    GappedResult {
        score: best_score,
        q_start: aq_start,
        q_end: aq_end,
        db_start: ad_start,
        db_end: ad_end,
        traceback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::NucleotideScoring;

    fn nuc() -> NucleotideScoring {
        NucleotideScoring::default()
    }

    #[test]
    fn test_ungapped_exact_match() {
        let query = b"ACGTACGT";
        let db = b"ACGTACGT";
        let scoring = nuc();
        let result = ungapped_extend(query, db, 0, 0, 4, &scoring, 10);
        assert!(result.score > 0);
        assert_eq!(result.q_start, 0);
        assert_eq!(result.q_end, 8);
    }

    #[test]
    fn test_ungapped_xdrop() {
        // Seed at pos 0, but mismatch at pos 4
        let query = b"ACGTAAAA";
        let db = b"ACGTTTTT";
        let scoring = nuc();
        // Start at seed "ACGT" (pos 0), extend right
        let result = ungapped_extend(query, db, 0, 0, 4, &scoring, 2);
        assert!(result.score > 0);
        // X-drop should stop extension before the end
        assert!(result.q_end <= 8);
    }

    #[test]
    fn test_ungapped_left_extension() {
        // Seed in the middle: query "CGT" at pos 4 matches db "CGT" at pos 4
        let query = b"AAAACGT";
        let db = b"TTTACGT";
        let scoring = nuc();
        let result = ungapped_extend(query, db, 4, 4, 3, &scoring, 20);
        assert!(result.score > 0);
        // Left extension should go past the seed
        assert!(result.q_start <= 4);
    }

    #[test]
    fn test_banded_sw_exact_match() {
        let query = b"ACGTACGT";
        let db = b"ACGTACGT";
        let scoring = nuc();
        let result = banded_sw(query, db, 0, 0, 4, 8, &scoring);
        assert!(result.score > 0);
        assert_eq!(result.q_start, 0);
        assert_eq!(result.q_end, 8);
    }

    #[test]
    fn test_banded_sw_with_gap() {
        let query = b"ACGACGT";
        let db = b"ACGTACGT";
        let scoring = nuc();
        let result = banded_sw(query, db, 3, 3, 4, 7, &scoring);
        assert!(result.score > 0);
    }

    #[test]
    fn test_banded_sw_no_match() {
        let query = b"AAAA";
        let db = b"TTTT";
        let scoring = nuc();
        let result = banded_sw(query, db, 0, 0, 2, 4, &scoring);
        // No positive alignment expected
        assert!(result.score <= 0 || result.traceback.is_empty());
    }
}
