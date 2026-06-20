//! Main search pipeline: orchestrates index, seed, extend, and stats.
//!
//! The search pipeline:
//! 1. Load database and build k-mer index.
//! 2. For each query:
//!    a. Find seed hits using the k-mer index.
//!    b. Cluster seeds along diagonals.
//!    c. Ungapped extension with X-drop.
//!    d. Gapped extension (banded SW) for surviving seeds.
//!    e. Compute alignment statistics.
//!    f. Report hits sorted by score.

use crate::extend::{banded_sw, ungapped_extend};
use crate::fasta::FastaRecord;
use crate::index::KmerIndex;
use crate::score::NucleotideScoring;
use crate::seed::{cluster_seeds, find_seeds};
use crate::stats::{compute_stats, AlignmentStats};

use anyhow::Result;

/// Configuration for a BLAST-like search.
#[derive(Debug, Clone)]
pub struct SearchConfig {
    /// Word / k-mer size.
    pub word_size: usize,
    /// X-drop threshold for ungapped extension.
    pub x_drop: i32,
    /// Band width for gapped extension.
    pub band_width: usize,
    /// Flank size for gapped extension.
    pub flank: usize,
    /// Maximum E-value threshold to report a hit.
    pub e_value_threshold: f64,
    /// Maximum number of hits to report.
    pub max_hits: usize,
    /// Match score.
    pub match_score: i32,
    /// Mismatch score.
    pub mismatch_score: i32,
    /// Gap open penalty.
    pub gap_open: i32,
    /// Gap extend penalty.
    pub gap_extend: i32,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            word_size: 11,
            x_drop: 10,
            band_width: 16,
            flank: 50,
            e_value_threshold: 10.0,
            max_hits: 500,
            match_score: 2,
            mismatch_score: -3,
            gap_open: 5,
            gap_extend: 2,
        }
    }
}

/// A single search hit with alignment details.
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// Database sequence index.
    pub db_seq_idx: usize,
    /// Database sequence header.
    pub db_header: String,
    /// Query alignment start (0-based, inclusive).
    pub query_start: usize,
    /// Query alignment end (0-based, exclusive).
    pub query_end: usize,
    /// Database alignment start (0-based, inclusive).
    pub db_start: usize,
    /// Database alignment end (0-based, exclusive).
    pub db_end: usize,
    /// Alignment statistics.
    pub stats: AlignmentStats,
    /// Alignment traceback: pairs of (query_pos, db_pos).
    pub traceback: Vec<(Option<usize>, Option<usize>)>,
    /// Number of independent seed clusters supporting this hit.
    /// Higher values indicate more evidence (e.g. multiple matching regions).
    pub seed_support: usize,
}

impl SearchHit {
    /// Format the alignment as a pairwise alignment string.
    pub fn format_alignment(&self, query: &[u8], db_seq: &[u8]) -> String {
        let mut output = String::new();

        output.push_str(&format!(
            "Query:  {}-{}\n",
            self.query_start + 1,
            self.query_end,
        ));
        output.push_str(&format!(
            "Sbjct:  {}  {}-{}\n",
            self.db_header,
            self.db_start + 1,
            self.db_end
        ));
        output.push_str(&format!(
            "Score:  {} bits ({:.1}), E-value: {:.2e}\n",
            self.stats.bit_score, self.stats.score, self.stats.e_value
        ));
        output.push_str(&format!(
            "Identity: {}/{} ({:.1}%)\n",
            self.stats.matches, self.stats.alignment_length, self.stats.percent_identity
        ));
        output.push('\n');

        // Build alignment lines from traceback
        let mut q_line = String::new();
        let mut mid_line = String::new();
        let mut s_line = String::new();

        for &(q_opt, d_opt) in &self.traceback {
            match (q_opt, d_opt) {
                (Some(qi), Some(di)) => {
                    let qc = query[qi] as char;
                    let dc = db_seq[di] as char;
                    q_line.push(qc);
                    mid_line.push(if qc == dc { '|' } else { ' ' });
                    s_line.push(dc);
                }
                (None, Some(_di)) => {
                    q_line.push('-');
                    mid_line.push(' ');
                    s_line.push(' ');
                }
                (Some(_qi), None) => {
                    q_line.push(' ');
                    mid_line.push(' ');
                    s_line.push('-');
                }
                (None, None) => {}
            }
        }

        output.push_str(&format!("Q {}\n", q_line));
        output.push_str(&format!("  {}\n", mid_line));
        output.push_str(&format!("S {}\n", s_line));

        output
    }

    /// Format hit as a tab-separated line.
    pub fn format_tabular(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{:.1}\t{:.2e}\t{}\t{}/{} ({:.1}%)",
            self.db_header,
            self.query_start + 1,
            self.query_end,
            self.db_start + 1,
            self.db_end,
            self.stats.score,
            self.stats.bit_score,
            self.stats.e_value,
            self.stats.alignment_length,
            self.stats.matches,
            self.stats.alignment_length,
            self.stats.percent_identity,
        )
    }
}

/// Run a BLAST-like search of a query against a database.
pub fn search(
    query: &FastaRecord,
    database: &[FastaRecord],
    index: &KmerIndex,
    config: &SearchConfig,
) -> Result<Vec<SearchHit>> {
    let scoring = NucleotideScoring {
        match_score: config.match_score,
        mismatch_score: config.mismatch_score,
        gap_open_penalty: config.gap_open,
        gap_extend_penalty: config.gap_extend,
    };

    let query_seq = query.as_bytes();

    // Total database size for E-value calculation
    let db_size: usize = database.iter().map(|r| r.len()).sum();

    // Step 1: Find seeds
    let seeds = find_seeds(query_seq, index);

    // Step 2: Cluster seeds
    let clusters = cluster_seeds(&seeds, config.band_width as i32);

    let mut raw_hits: Vec<SearchHit> = Vec::new();

    // Step 3: For each cluster, do ungapped then gapped extension
    for cluster in &clusters {
        if cluster.is_empty() {
            continue;
        }

        // Pick representative seeds from the cluster (spread them out)
        let representative = &cluster[0];

        let db_rec = &database[representative.db_seq_idx];
        let db_seq = db_rec.as_bytes();

        // Ungapped extension
        let ungapped = ungapped_extend(
            query_seq,
            db_seq,
            representative.query_pos,
            representative.db_pos,
            config.word_size,
            &scoring,
            config.x_drop,
        );

        // Only proceed if ungapped extension found a positive score
        if ungapped.score <= 0 {
            continue;
        }

        // Gapped extension from the ungapped region center
        let center_q = (ungapped.q_start + ungapped.q_end) / 2;
        let center_db = (ungapped.db_start + ungapped.db_end) / 2;

        let gapped = banded_sw(
            query_seq,
            db_seq,
            center_q,
            center_db,
            config.band_width,
            config.flank,
            &scoring,
        );

        if gapped.score <= 0 {
            continue;
        }

        // Compute alignment statistics
        let stats = compute_stats(
            &gapped.traceback,
            query_seq,
            db_seq,
            &scoring,
            db_size,
            query_seq.len(),
        );

        // Filter by E-value
        if stats.e_value > config.e_value_threshold {
            continue;
        }

        raw_hits.push(SearchHit {
            db_seq_idx: representative.db_seq_idx,
            db_header: db_rec.header.clone(),
            query_start: gapped.q_start,
            query_end: gapped.q_end,
            db_start: gapped.db_start,
            db_end: gapped.db_end,
            stats,
            traceback: gapped.traceback,
            seed_support: 1,
        });
    }

    // Step 4: Merge overlapping hits for the same db sequence
    let merged = merge_hits(raw_hits);

    // Step 5: Sort by score (descending), then seed_support (descending) as tie-breaker
    let mut sorted = merged;
    sorted.sort_by(|a, b| {
        b.stats
            .score
            .cmp(&a.stats.score)
            .then(b.seed_support.cmp(&a.seed_support))
    });
    sorted.truncate(config.max_hits);

    Ok(sorted)
}

/// Merge overlapping hits on the same database sequence.
/// When overlapping hits are merged, seed_support counts are summed
/// to reflect the total evidence from independent seed clusters.
fn merge_hits(hits: Vec<SearchHit>) -> Vec<SearchHit> {
    if hits.is_empty() {
        return hits;
    }

    let mut sorted = hits;
    sorted.sort_by_key(|h| (h.db_seq_idx, h.query_start));

    let mut groups: Vec<Vec<SearchHit>> = Vec::new();
    let mut current_group: Vec<SearchHit> = vec![sorted.remove(0)];

    while !sorted.is_empty() {
        let hit = sorted.remove(0);
        let last = current_group.last().unwrap();
        if hit.db_seq_idx == last.db_seq_idx && hit.query_start <= last.query_end {
            // Overlapping — keep the better one, accumulate seed_support
            if hit.stats.score > last.stats.score {
                let mut kept = current_group.pop().unwrap();
                kept.seed_support += hit.seed_support;
                current_group.push(kept);
            } else {
                current_group.last_mut().unwrap().seed_support += hit.seed_support;
            }
        } else {
            groups.push(std::mem::take(&mut current_group));
            current_group = vec![hit];
        }
    }
    groups.push(current_group);

    groups.into_iter().map(|g| g.into_iter().next().unwrap()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_records(seqs: &[(&str, &str)]) -> Vec<FastaRecord> {
        seqs.iter()
            .map(|(hdr, seq)| FastaRecord {
                header: hdr.to_string(),
                seq: seq.as_bytes().to_vec(),
            })
            .collect()
    }

    #[test]
    fn test_search_exact_match() {
        let db = make_records(&[("db_seq", "ACGTACGTACGTACGTACGT")]);
        let idx = KmerIndex::build(&db, 4);
        let config = SearchConfig {
            word_size: 4,
            ..Default::default()
        };
        let query = FastaRecord {
            header: "q".to_string(),
            seq: b"ACGTACGT".to_vec(),
        };

        let results = search(&query, &db, &idx, &config).unwrap();
        assert!(!results.is_empty(), "Should find at least one hit");
        assert!(results[0].stats.score > 0);
    }

    #[test]
    fn test_search_no_match() {
        let db = make_records(&[("db_seq", "TTTTTTTTTTTTTTTTTTTT")]);
        let idx = KmerIndex::build(&db, 4);
        let config = SearchConfig {
            word_size: 4,
            ..Default::default()
        };
        let query = FastaRecord {
            header: "q".to_string(),
            seq: b"ACGTACGT".to_vec(),
        };

        let results = search(&query, &db, &idx, &config).unwrap();
        assert!(results.is_empty(), "Should find no hits");
    }

    #[test]
    fn test_search_partial_match() {
        let db = make_records(&[("db_seq", "ACGTACGTACGTACGT")]);
        let idx = KmerIndex::build(&db, 4);
        let config = SearchConfig {
            word_size: 4,
            e_value_threshold: 100.0, // relax threshold
            ..Default::default()
        };
        let query = FastaRecord {
            header: "q".to_string(),
            seq: b"ACGTACGT".to_vec(),
        };

        let results = search(&query, &db, &idx, &config).unwrap();
        assert!(!results.is_empty());
        // Check we got alignment statistics
        assert!(results[0].stats.alignment_length > 0);
    }

    #[test]
    fn test_search_multi_db() {
        let db = make_records(&[
            ("seq1", "ACGTACGTACGTACGT"),
            ("seq2", "TTTTTTTTTTTTTTTT"),
            ("seq3", "ACGTACGT"),
        ]);
        let idx = KmerIndex::build(&db, 4);
        let config = SearchConfig {
            word_size: 4,
            ..Default::default()
        };
        let query = FastaRecord {
            header: "q".to_string(),
            seq: b"ACGTACGT".to_vec(),
        };

        let results = search(&query, &db, &idx, &config).unwrap();
        // Should find hits in seq1 and seq3, but not seq2
        assert!(!results.is_empty());
    }

    #[test]
    fn test_search_hit_sorting() {
        let db = make_records(&[
            ("short", "ACGTACGT"),
            ("long", "ACGTACGTACGTACGTACGTACGT"),
        ]);
        let idx = KmerIndex::build(&db, 4);
        let config = SearchConfig {
            word_size: 4,
            e_value_threshold: 1000.0,
            ..Default::default()
        };
        let query = FastaRecord {
            header: "q".to_string(),
            seq: b"ACGTACGT".to_vec(),
        };

        let results = search(&query, &db, &idx, &config).unwrap();
        if results.len() > 1 {
            // Should be sorted by score descending
            for i in 1..results.len() {
                assert!(results[i - 1].stats.score >= results[i].stats.score);
            }
        }
    }

    #[test]
    fn test_tabular_output() {
        let hit = SearchHit {
            db_seq_idx: 0,
            db_header: "test_seq".to_string(),
            query_start: 0,
            query_end: 10,
            db_start: 5,
            db_end: 15,
            stats: AlignmentStats {
                score: 20,
                alignment_length: 10,
                matches: 9,
                mismatches: 1,
                gap_opens: 0,
                gap_extensions: 0,
                percent_identity: 90.0,
                e_value: 1e-5,
                bit_score: 12.5,
            },
            traceback: Vec::new(),
            seed_support: 1,
        };
        let tab = hit.format_tabular();
        assert!(tab.contains("test_seq"));
        assert!(tab.contains("90.0%"));
    }

    #[test]
    fn test_config_defaults() {
        let config = SearchConfig::default();
        assert_eq!(config.word_size, 11);
        assert_eq!(config.x_drop, 10);
    }
}
