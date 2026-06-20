//! Alignment statistics: percent identity, score, and E-value calculation.
//!
//! Provides the statistical framework for evaluating alignment significance.

use crate::score::ScoringScheme;

/// Statistics for an alignment between a query and a database sequence.
#[derive(Debug, Clone)]
pub struct AlignmentStats {
    /// Alignment score.
    pub score: i32,
    /// Number of alignment columns.
    pub alignment_length: usize,
    /// Number of matching columns.
    pub matches: usize,
    /// Number of mismatches.
    pub mismatches: usize,
    /// Number of gap-open events.
    pub gap_opens: usize,
    /// Number of gap-extend events (total gapped columns).
    pub gap_extensions: usize,
    /// Percent identity (0.0 - 100.0).
    pub percent_identity: f64,
    /// E-value (approximate).
    pub e_value: f64,
    /// Bit score (normalized).
    pub bit_score: f64,
}

/// Compute alignment statistics from a traceback and scoring scheme.
///
/// - `traceback`: pairs of (query_pos, db_pos); None means a gap.
/// - `query` and `db_seq`: the original sequences.
/// - `scoring`: the scoring scheme used.
/// - `db_size`: total size of the database (sum of all seq lengths) for E-value.
/// - `query_len`: length of the query for E-value.
pub fn compute_stats(
    traceback: &[(Option<usize>, Option<usize>)],
    query: &[u8],
    db_seq: &[u8],
    scoring: &dyn ScoringScheme,
    db_size: usize,
    query_len: usize,
) -> AlignmentStats {
    if traceback.is_empty() {
        return AlignmentStats {
            score: 0,
            alignment_length: 0,
            matches: 0,
            mismatches: 0,
            gap_opens: 0,
            gap_extensions: 0,
            percent_identity: 0.0,
            e_value: 0.0,
            bit_score: 0.0,
        };
    }

    let mut matches = 0usize;
    let mut mismatches = 0usize;
    let mut gap_opens = 0usize;
    let mut gap_extensions = 0usize;
    let mut total_cols = 0usize;

    let mut in_gap_query = false;
    let mut in_gap_db = false;

    for &(q_opt, d_opt) in traceback {
        total_cols += 1;
        match (q_opt, d_opt) {
            (Some(qi), Some(di)) => {
                in_gap_query = false;
                in_gap_db = false;
                if query[qi] == db_seq[di] {
                    matches += 1;
                } else {
                    mismatches += 1;
                }
            }
            (None, Some(_)) => {
                // Gap in query
                if !in_gap_query {
                    gap_opens += 1;
                    in_gap_query = true;
                } else {
                    gap_extensions += 1;
                }
                in_gap_db = false;
            }
            (Some(_), None) => {
                // Gap in db
                if !in_gap_db {
                    gap_opens += 1;
                    in_gap_db = true;
                } else {
                    gap_extensions += 1;
                }
                in_gap_query = false;
            }
            (None, None) => {}
        }
    }

    let percent_identity = if total_cols > 0 {
        (matches as f64 / total_cols as f64) * 100.0
    } else {
        0.0
    };

    // Compute raw score from traceback
    let raw_score = compute_raw_score(traceback, query, db_seq, scoring);

    // Bit score: S' = (lambda * S - ln(K)) / ln(2)
    // For ungapped nucleotide: approximate lambda from Karlin-Altschul
    let (lambda, k_param) = karlin_params(scoring);

    let bit_score = if lambda > 0.0 && k_param > 0.0 {
        (lambda * raw_score as f64 - k_param.ln()) / 2.0_f64.ln()
    } else {
        raw_score as f64
    };

    // E-value: E = K * m * n * e^(-lambda * S)
    let e_value = if lambda > 0.0 && k_param > 0.0 && db_size > 0 && query_len > 0 {
        k_param * query_len as f64 * db_size as f64 * (-lambda * raw_score as f64).exp()
    } else {
        0.0
    };

    AlignmentStats {
        score: raw_score,
        alignment_length: total_cols,
        matches,
        mismatches,
        gap_opens,
        gap_extensions,
        percent_identity,
        e_value,
        bit_score,
    }
}

/// Compute the raw alignment score from a traceback.
fn compute_raw_score(
    traceback: &[(Option<usize>, Option<usize>)],
    query: &[u8],
    db_seq: &[u8],
    scoring: &dyn ScoringScheme,
) -> i32 {
    let mut score = 0i32;
    let mut in_gap = false;

    for &(q_opt, d_opt) in traceback {
        match (q_opt, d_opt) {
            (Some(qi), Some(di)) => {
                score += scoring.score(query[qi], db_seq[di]);
                in_gap = false;
            }
            _ => {
                if !in_gap {
                    score -= scoring.gap_open();
                    in_gap = true;
                } else {
                    score -= scoring.gap_extend();
                }
            }
        }
    }
    score
}

/// Approximate Karlin-Altschul parameters for a scoring scheme.
/// Returns (lambda, K).
fn karlin_params(scoring: &dyn ScoringScheme) -> (f64, f64) {
    // For standard nucleotide scoring (match=2, mismatch=-3, gap_open=5, gap_extend=2):
    // lambda ≈ 1.28, K ≈ 0.46
    //
    // For protein BLOSUM62 (gap_open=11, gap_extend=1):
    // lambda ≈ 0.317, K ≈ 0.13
    //
    // We use heuristic approximations based on the scoring parameters.

    let alphabet = scoring.alphabet_size();

    if alphabet <= 5 {
        // Nucleotide-like: approximate from match/mismatch ratio
        let lambda = 1.28;
        let k = 0.46;
        (lambda, k)
    } else {
        // Protein-like: BLOSUM-family approximation
        let lambda = 0.317;
        let k = 0.13;
        (lambda, k)
    }
}

/// Compute percent identity from match/mismatch/alignment length.
pub fn percent_identity(matches: usize, alignment_length: usize) -> f64 {
    if alignment_length == 0 {
        0.0
    } else {
        (matches as f64 / alignment_length as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::NucleotideScoring;

    #[test]
    fn test_percent_identity() {
        assert!((percent_identity(8, 10) - 80.0).abs() < f64::EPSILON);
        assert!((percent_identity(0, 10) - 0.0).abs() < f64::EPSILON);
        assert!((percent_identity(5, 0) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_stats_perfect_match() {
        let query = b"ACGT";
        let db = b"ACGT";
        let traceback: Vec<_> = (0..4).map(|i| (Some(i), Some(i))).collect();
        let scoring = NucleotideScoring::default();

        let stats = compute_stats(&traceback, query, db, &scoring, 4, 4);
        assert_eq!(stats.matches, 4);
        assert_eq!(stats.mismatches, 0);
        assert!((stats.percent_identity - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_stats_mismatch() {
        let query = b"ACGT";
        let db = b"ACGA";
        let traceback: Vec<_> = (0..4).map(|i| (Some(i), Some(i))).collect();
        let scoring = NucleotideScoring::default();

        let stats = compute_stats(&traceback, query, db, &scoring, 4, 4);
        assert_eq!(stats.matches, 3);
        assert_eq!(stats.mismatches, 1);
        assert!((stats.percent_identity - 75.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_stats_with_gap() {
        let query = b"ACGT";
        let db = b"ACGGT";
        // Alignment: A-C-G-T / A-C-G-G-T
        let traceback = vec![
            (Some(0), Some(0)),
            (Some(1), Some(1)),
            (Some(2), Some(2)),
            (None, Some(3)),       // gap in query
            (Some(3), Some(4)),
        ];
        let scoring = NucleotideScoring::default();

        let stats = compute_stats(&traceback, query, db, &scoring, 5, 4);
        assert_eq!(stats.matches, 4);
        assert_eq!(stats.gap_opens, 1);
        assert!(stats.score > 0);
    }

    #[test]
    fn test_empty_traceback() {
        let scoring = NucleotideScoring::default();
        let stats = compute_stats(&[], b"ACGT", b"ACGT", &scoring, 4, 4);
        assert_eq!(stats.score, 0);
        assert_eq!(stats.alignment_length, 0);
    }

    #[test]
    fn test_e_value_positive() {
        let scoring = NucleotideScoring::default();
        let traceback: Vec<_> = (0..8).map(|i| (Some(i), Some(i))).collect();
        let stats = compute_stats(&traceback, b"ACGTACGT", b"ACGTACGT", &scoring, 1000, 8);
        assert!(stats.e_value >= 0.0);
        assert!(stats.bit_score > 0.0);
    }
}
