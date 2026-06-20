//! Integration tests for bio-fasta-fastq-toolkit.
//!
//! These tests exercise the full pipeline: parsing → stats → conversion,
//! using small embedded test data that covers edge cases.

use std::fs;
use bio_fasta_fastq_toolkit::fasta;
use bio_fasta_fastq_toolkit::fastq;
use bio_fasta_fastq_toolkit::stats;
use bio_fasta_fastq_toolkit::quality::{self, QualityEncoding};
use bio_fasta_fastq_toolkit::convert;
use bio_fasta_fastq_toolkit::seqops;

// ---------------------------------------------------------------------------
// Embedded test data
// ---------------------------------------------------------------------------

const FASTA_SIMPLE: &[u8] = b">seq1 first sequence\nACGTACGT\n>seq2 second\nTTTTGGGG\n";

const FASTA_EMPTY: &[u8] = b"";

const FASTA_SINGLE: &[u8] = b">only\nACGTN\n";

const FASTA_WRAPPED: &[u8] = b">wrap long sequence\nACGT\nTGCA\nAAAA\nGGGG\n";

const FASTA_LOWERCASE: &[u8] = b">lc\nacgt\nacgt\n";

const FASTQ_SIMPLE: &[u8] = b"@read1 desc\nACGT\n+\nIIII\n@read2\nTTTT\n+\n!!!!\n";

const FASTQ_EMPTY: &[u8] = b"";

const FASTQ_BAD_QUAL_LEN: &[u8] = b"@bad\nACGT\n+\nII\n";

const FASTQ_SINGLE: &[u8] = b"@solo\nACGTN\n+\n!!!!!\n";

// ---------------------------------------------------------------------------
// FASTA integration tests
// ---------------------------------------------------------------------------

#[test]
fn test_fasta_end_to_end() {
    let records: Vec<_> = fasta::parse_reader(FASTA_SIMPLE)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(records.len(), 2);

    // Verify record structure
    assert_eq!(records[0].id, "seq1");
    assert_eq!(records[0].description, "first sequence");
    assert_eq!(records[0].sequence, "ACGTACGT");
    assert_eq!(records[1].id, "seq2");
    assert_eq!(records[1].sequence, "TTTTGGGG");

    // Stats
    let lengths: Vec<usize> = records.iter().map(|r| r.len()).collect();
    let ls = stats::length_stats(&lengths);
    assert_eq!(ls.count, 2);
    assert_eq!(ls.total_bases, 16);
    assert_eq!(ls.n50, 8); // both sequences are 8, so N50 = 8

    let sequences: Vec<&str> = records.iter().map(|r| r.sequence.as_str()).collect();
    let comp = stats::aggregate_composition(&sequences);
    assert_eq!(comp.a, 2); // ACGTACGT has 2A, TTTTGGGG has 0A → total 2
    // ACGTACGT: A=2, C=2, G=2, T=2
    // TTTTGGGG: A=0, C=0, G=4, T=4
    // Total: A=2, C=2, G=6, T=6
    assert_eq!(comp.a, 2);
    assert_eq!(comp.c, 2);
    assert_eq!(comp.g, 6);
    assert_eq!(comp.t, 6);
}

#[test]
fn test_fasta_empty_file() {
    let records: Vec<_> = fasta::parse_reader(FASTA_EMPTY)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(records.is_empty());
}

#[test]
fn test_fasta_single_record() {
    let records: Vec<_> = fasta::parse_reader(FASTA_SINGLE)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].sequence, "ACGTN");
}

#[test]
fn test_fasta_wrapped_lines() {
    let records: Vec<_> = fasta::parse_reader(FASTA_WRAPPED)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].sequence, "ACGTTGCAA AAAGGGG".replace(' ', ""));
    assert_eq!(records[0].len(), 16);
}

#[test]
fn test_fasta_lowercase() {
    let records: Vec<_> = fasta::parse_reader(FASTA_LOWERCASE)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(records[0].sequence, "ACGTACGT");
}

// ---------------------------------------------------------------------------
// FASTQ integration tests
// ---------------------------------------------------------------------------

#[test]
fn test_fastq_end_to_end() {
    let records: Vec<_> = fastq::parse_reader(FASTQ_SIMPLE)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].id, "read1");
    assert_eq!(records[0].quality, "IIII");
}

#[test]
fn test_fastq_empty_file() {
    let records: Vec<_> = fastq::parse_reader(FASTQ_EMPTY)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(records.is_empty());
}

#[test]
fn test_fastq_single_record() {
    let records: Vec<_> = fastq::parse_reader(FASTQ_SINGLE)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].id, "solo");
}

#[test]
fn test_fastq_bad_qual_length() {
    let result: Result<Vec<_>, _> = fastq::parse_reader(FASTQ_BAD_QUAL_LEN).collect();
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("Quality length"), "Error message: {}", msg);
}

// ---------------------------------------------------------------------------
// Quality integration tests
// ---------------------------------------------------------------------------

#[test]
fn test_quality_filter_pipeline() {
    let records: Vec<_> = fastq::parse_reader(FASTQ_SIMPLE)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(records.len(), 2);

    let filtered = quality::filter_by_quality(records, 20.0, QualityEncoding::Sanger).unwrap();
    assert_eq!(filtered.len(), 1); // Only read1 (mean=40) survives, read2 (mean=0) filtered
    assert_eq!(filtered[0].id, "read1");
}

#[test]
fn test_quality_trim_pipeline() {
    let records: Vec<_> = fastq::parse_reader(FASTQ_SIMPLE)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let trimmed = quality::trim_records(records, 4, 20.0, QualityEncoding::Sanger).unwrap();
    // read1: all quality 40, no trimming
    // read2: all quality 0, entire read trimmed → removed
    assert_eq!(trimmed.len(), 1);
    assert_eq!(trimmed[0].id, "read1");
    assert_eq!(trimmed[0].sequence, "ACGT");
}

// ---------------------------------------------------------------------------
// Conversion integration tests
// ---------------------------------------------------------------------------

#[test]
fn test_conversion_pipeline() {
    let mut output = Vec::new();
    let count = convert::fastq_to_fasta(FASTQ_SIMPLE, &mut output).unwrap();
    assert_eq!(count, 2);

    let fasta_str = String::from_utf8(output).unwrap();
    let records: Vec<_> = fasta::parse_reader(fasta_str.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].id, "read1");
    assert_eq!(records[0].sequence, "ACGT");
    assert_eq!(records[1].id, "read2");
}

// ---------------------------------------------------------------------------
// Seqops integration tests
// ---------------------------------------------------------------------------

#[test]
fn test_reverse_complement_integration() {
    let records: Vec<_> = fasta::parse_reader(FASTA_SIMPLE)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let rc = seqops::reverse_complement(&records[0].sequence).unwrap();
    assert_eq!(rc, "ACGTACGT"); // Palindrome: ACGTACGT rev-comp = ACGTACGT
}

#[test]
fn test_translate_integration() {
    // ATG GCT GGT = M A G
    let protein = seqops::translate("ATGGCTGGT").unwrap();
    assert_eq!(protein, "MAG");
}

#[test]
fn test_subsample_integration() {
    let records: Vec<_> = fasta::parse_reader(FASTA_SIMPLE)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    // With fraction=1.0, should keep all
    let sampled = seqops::subsample(records.clone(), 1.0);
    assert_eq!(sampled.len(), 2);

    // With fraction=0.0, should keep none
    let sampled = seqops::subsample(records, 0.0);
    assert!(sampled.is_empty());
}

// ---------------------------------------------------------------------------
// Stats integration tests
// ---------------------------------------------------------------------------

#[test]
fn test_n50_calculation_on_real_data() {
    let records: Vec<_> = fasta::parse_reader(FASTA_SIMPLE)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let lengths: Vec<usize> = records.iter().map(|r| r.len()).collect();
    let ls = stats::length_stats(&lengths);
    // Both sequences are 8bp. Total = 16. Half = 8.
    // Sorted desc: [8, 8]. Cumulative after first: 8 >= 8 → N50=8, L50=1
    assert_eq!(ls.n50, 8);
    assert_eq!(ls.l50, 1);
    assert_eq!(ls.mean, 8.0);
    assert_eq!(ls.median, 8.0);
}

// ---------------------------------------------------------------------------
// File I/O tests (using temp files)
// ---------------------------------------------------------------------------

#[test]
fn test_fasta_file_io() {
    let dir = std::env::temp_dir().join("bio_toolkit_test");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.fasta");
    fs::write(&path, FASTA_SIMPLE).unwrap();

    let records = fasta::parse_to_vec(path.to_str().unwrap()).unwrap();
    assert_eq!(records.len(), 2);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_fastq_file_io() {
    let dir = std::env::temp_dir().join("bio_toolkit_test_fq");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.fastq");
    fs::write(&path, FASTQ_SIMPLE).unwrap();

    let records = fastq::parse_to_vec(path.to_str().unwrap()).unwrap();
    assert_eq!(records.len(), 2);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_convert_file_io() {
    let dir = std::env::temp_dir().join("bio_toolkit_test_convert");
    fs::create_dir_all(&dir).unwrap();
    let in_path = dir.join("in.fastq");
    let out_path = dir.join("out.fasta");
    fs::write(&in_path, FASTQ_SIMPLE).unwrap();

    let count = convert::convert_file(
        in_path.to_str().unwrap(),
        out_path.to_str().unwrap(),
    ).unwrap();
    assert_eq!(count, 2);

    let records = fasta::parse_to_vec(out_path.to_str().unwrap()).unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].id, "read1");

    fs::remove_dir_all(&dir).ok();
}
