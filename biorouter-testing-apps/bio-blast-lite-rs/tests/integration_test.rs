//! Integration tests for bio-blast-lite-rs.
//!
//! Tests the full pipeline from FASTA parsing through search results.

use bio_blast_lite_rs::fasta::{parse_fasta_file, FastaRecord};
use bio_blast_lite_rs::index::KmerIndex;
use bio_blast_lite_rs::search::{search, SearchConfig};
use bio_blast_lite_rs::seed::find_seeds;

use std::io::Write;
use tempfile::NamedTempFile;

// ============================================================================
// Helper: write FASTA to a temp file
// ============================================================================

fn write_temp_fasta(records: &[(&str, &str)]) -> NamedTempFile {
    let mut f = NamedTempFile::new().expect("create temp file");
    for (hdr, seq) in records {
        writeln!(f, ">{}", hdr).unwrap();
        // Write in 80-char lines
        for chunk in seq.as_bytes().chunks(80) {
            f.write_all(chunk).unwrap();
            writeln!(f).unwrap();
        }
    }
    f.flush().unwrap();
    f
}

fn make_records(seqs: &[(&str, &str)]) -> Vec<FastaRecord> {
    seqs.iter()
        .map(|(hdr, seq)| FastaRecord {
            header: hdr.to_string(),
            seq: seq.as_bytes().to_vec(),
        })
        .collect()
}

// ============================================================================
// Test: Exact match found
// ============================================================================

#[test]
fn integration_exact_match_found() {
    let db = make_records(&[("db1", "ACGTACGTACGTACGTACGT")]);
    let idx = KmerIndex::build(&db, 11);
    let config = SearchConfig {
        word_size: 11,
        ..Default::default()
    };
    let query = FastaRecord {
        header: "q1".to_string(),
        seq: b"ACGTACGTACGT".to_vec(),
    };

    let results = search(&query, &db, &idx, &config).unwrap();
    assert!(!results.is_empty(), "Should find a hit for exact match");
    assert!(results[0].stats.percent_identity >= 99.0);
}

// ============================================================================
// Test: No match found
// ============================================================================

#[test]
fn integration_no_match_found() {
    let db = make_records(&[("db_polyA", "AAAAAAAAAAAAAAAAAAAA")]);
    let idx = KmerIndex::build(&db, 11);
    let config = SearchConfig {
        word_size: 11,
        e_value_threshold: 10.0,
        ..Default::default()
    };
    let query = FastaRecord {
        header: "q1".to_string(),
        seq: b"TTTTTTTTTTTT".to_vec(),
    };

    let results = search(&query, &db, &idx, &config).unwrap();
    assert!(results.is_empty(), "Should find no hits for poly-A vs poly-T");
}

// ============================================================================
// Test: Known alignment on small sequences
// ============================================================================

#[test]
fn integration_known_alignment() {
    // Query has a perfect 12-mer match to db at a known location
    let db = make_records(&[("db_known", "TTTTTTACGTACGTACGTTTTTTT")]);
    let idx = KmerIndex::build(&db, 11);
    let config = SearchConfig {
        word_size: 11,
        e_value_threshold: 100.0,
        ..Default::default()
    };
    let query = FastaRecord {
        header: "q_known".to_string(),
        seq: b"ACGTACGTACGT".to_vec(),
    };

    let results = search(&query, &db, &idx, &config).unwrap();
    assert!(!results.is_empty(), "Should find a hit for known alignment");

    let hit = &results[0];
    // The alignment should span roughly positions 8-20 of the db
    assert!(hit.db_start >= 6 && hit.db_start <= 12);
    assert!(hit.stats.score > 0);
}

// ============================================================================
// Test: Seed-extension correctness
// ============================================================================

#[test]
fn integration_seed_extension_correctness() {
    let db = make_records(&[("db_ext", "CCACGTACGTACGTCCCC")]);
    let idx = KmerIndex::build(&db, 4);
    let config = SearchConfig {
        word_size: 4,
        x_drop: 10,
        flank: 20,
        e_value_threshold: 1000.0,
        ..Default::default()
    };
    let query = FastaRecord {
        header: "q_ext".to_string(),
        seq: b"ACGTACGT".to_vec(),
    };

    // First verify seeds are found
    let seeds = find_seeds(query.as_bytes(), &idx);
    assert!(!seeds.is_empty(), "Should find seed hits");

    // Now run full search
    let results = search(&query, &db, &idx, &config).unwrap();
    assert!(!results.is_empty(), "Should find a hit after extension");

    // The alignment should have extended beyond the seed
    let hit = &results[0];
    assert!(hit.query_end - hit.query_start >= 4, "Alignment should be >= seed size");
}

// ============================================================================
// Test: Multi-hit ranking
// ============================================================================

#[test]
fn integration_multi_hit_ranking() {
    // Database has two sequences: one perfect match, one partial
    let db = make_records(&[
        ("perfect", "ACGTACGTACGTACGTACGT"),
        ("partial", "ACGTACGTTTTTTTTTTTT"),
    ]);
    let idx = KmerIndex::build(&db, 4);
    let config = SearchConfig {
        word_size: 4,
        e_value_threshold: 1000.0,
        ..Default::default()
    };
    let query = FastaRecord {
        header: "q_multi".to_string(),
        seq: b"ACGTACGT".to_vec(),
    };

    let results = search(&query, &db, &idx, &config).unwrap();

    if results.len() >= 2 {
        // Results should be sorted by score (descending)
        assert!(
            results[0].stats.score >= results[1].stats.score,
            "First hit should have >= score than second"
        );
    }
}

// ============================================================================
// Test: FASTA file I/O
// ============================================================================

#[test]
fn integration_fasta_file_io() {
    let records = vec![
        ("seq1 test sequence", "ACGTACGTACGT"),
        ("seq2 another seq", "TTTTCCCCGGGG"),
    ];

    let temp = write_temp_fasta(&records);
    let parsed = parse_fasta_file(temp.path()).unwrap();

    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].id(), "seq1");
    assert_eq!(parsed[0].seq, b"ACGTACGTACGT");
    assert_eq!(parsed[1].id(), "seq2");
    assert_eq!(parsed[1].seq, b"TTTTCCCCGGGG");
}

// ============================================================================
// Test: Large database performance
// ============================================================================

#[test]
fn integration_large_database() {
    // Create a moderately large database (50 sequences of length 1000)
    let mut db_recs: Vec<(String, String)> = Vec::new();
    let mut rng_state: u32 = 42;
    for i in 0..50 {
        let mut seq = String::with_capacity(1000);
        for _ in 0..1000 {
            rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            let base = match rng_state % 4 {
                0 => 'A',
                1 => 'C',
                2 => 'G',
                _ => 'T',
            };
            seq.push(base);
        }
        let header = format!("seq_{}", i);
        db_recs.push((header, seq));
    }

    // Insert a known sequence at a known location
    let known_seq = "ACGTACGTACGTACGTACGT";
    // Put it at position 500 in sequence 25
    let seq25 = db_recs[25].1.clone();
    let mut modified = seq25[..500].to_string();
    modified.push_str(known_seq);
    modified.push_str(&seq25[520..]);
    db_recs[25].1 = modified;

    let db: Vec<FastaRecord> = db_recs
        .iter()
        .map(|(h, s)| FastaRecord {
            header: h.to_string(),
            seq: s.as_bytes().to_vec(),
        })
        .collect();

    let idx = KmerIndex::build(&db, 11);
    let config = SearchConfig {
        word_size: 11,
        e_value_threshold: 100.0,
        ..Default::default()
    };
    let query = FastaRecord {
        header: "q_large".to_string(),
        seq: known_seq.as_bytes().to_vec(),
    };

    let results = search(&query, &db, &idx, &config).unwrap();
    assert!(!results.is_empty(), "Should find the known sequence");

    // The best hit should be from sequence 25
    let best = &results[0];
    assert_eq!(best.db_header, "seq_25");
}

// ============================================================================
// Test: Configurable parameters
// ============================================================================

#[test]
fn integration_configurable_word_size() {
    let db = make_records(&[("db_config", "ACGTACGTACGTACGT")]);
    let query = FastaRecord {
        header: "q".to_string(),
        seq: b"ACGTACGT".to_vec(),
    };

    // With k=4, many seeds
    let idx4 = KmerIndex::build(&db, 4);
    let seeds4 = find_seeds(query.as_bytes(), &idx4);

    // With k=11, fewer seeds
    let idx11 = KmerIndex::build(&db, 11);
    let seeds11 = find_seeds(query.as_bytes(), &idx11);

    // k=4 should produce more seeds than k=11
    assert!(
        seeds4.len() >= seeds11.len(),
        "Smaller k should produce more or equal seeds"
    );
}

// ============================================================================
// Test: E-value filtering
// ============================================================================

#[test]
fn integration_evalue_filtering() {
    let db = make_records(&[("db_ev", "ACGTACGTACGTACGT")]);
    let idx = KmerIndex::build(&db, 4);

    // Very strict e-value threshold
    let config_strict = SearchConfig {
        word_size: 4,
        e_value_threshold: 1e-100,
        ..Default::default()
    };

    // Very permissive e-value threshold
    let config_loose = SearchConfig {
        word_size: 4,
        e_value_threshold: 1e3,
        ..Default::default()
    };

    let query = FastaRecord {
        header: "q".to_string(),
        seq: b"ACGTACGT".to_vec(),
    };

    let results_strict = search(&query, &db, &idx, &config_strict).unwrap();
    let results_loose = search(&query, &db, &idx, &config_loose).unwrap();

    // Strict should have <= results than loose
    assert!(
        results_strict.len() <= results_loose.len(),
        "Strict e-value should filter more"
    );
}
