//! FASTA sequence parsing for multi-record files.

use anyhow::{Context, Result};
use std::fmt;
use std::fs;
use std::io::{self, BufRead, BufReader, Read};
use std::path::Path;

/// A single FASTA record: header + raw sequence (no whitespace/newlines).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastaRecord {
    /// The full header line without the leading '>'.
    pub header: String,
    /// The concatenated sequence (uppercase, no whitespace).
    pub seq: Vec<u8>,
}

impl FastaRecord {
    /// Access the raw sequence bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.seq
    }

    /// Length of the sequence.
    pub fn len(&self) -> usize {
        self.seq.len()
    }

    /// Whether the sequence is empty.
    pub fn is_empty(&self) -> bool {
        self.seq.is_empty()
    }

    /// Short display id (first whitespace-delimited token of the header).
    pub fn id(&self) -> &str {
        self.header.split_whitespace().next().unwrap_or(&self.header)
    }
}

impl fmt::Display for FastaRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, ">{}\n", self.header)?;
        // Print sequence in 80-char lines
        for chunk in self.seq.chunks(80) {
            let s = std::str::from_utf8(chunk).unwrap_or("?");
            writeln!(f, "{}", s)?;
        }
        Ok(())
    }
}

/// Parse all FASTA records from a reader.
pub fn parse_fasta_reader<R: Read>(reader: R) -> Result<Vec<FastaRecord>> {
    let buf = BufReader::new(reader);
    let mut records = Vec::new();
    let mut current_header: Option<String> = None;
    let mut current_seq: Vec<u8> = Vec::new();

    for line_result in buf.lines() {
        let line = line_result.context("Failed to read line from FASTA input")?;
        let trimmed = line.trim();

        if trimmed.starts_with('>') {
            // Save previous record
            if let Some(hdr) = current_header.take() {
                records.push(FastaRecord {
                    header: hdr,
                    seq: std::mem::take(&mut current_seq),
                });
            }
            current_header = Some(trimmed[1..].to_string());
        } else if !trimmed.is_empty() {
            // Accumulate sequence characters (strip whitespace, uppercase)
            for ch in trimmed.bytes() {
                match ch {
                    b' ' | b'\t' | b'\r' | b'\n' => {}        // skip whitespace
                    b'.' => {}                               // gaps
                    _ => current_seq.push(ch.to_ascii_uppercase()),
                }
            }
        }
    }

    // Don't forget the last record
    if let Some(hdr) = current_header {
        records.push(FastaRecord {
            header: hdr,
            seq: current_seq,
        });
    }

    Ok(records)
}

/// Parse all FASTA records from a file path.
pub fn parse_fasta_file<P: AsRef<Path>>(path: P) -> Result<Vec<FastaRecord>> {
    let path = path.as_ref();
    let file = fs::File::open(path)
        .with_context(|| format!("Failed to open FASTA file: {}", path.display()))?;
    parse_fasta_reader(file).with_context(|| format!("Failed to parse FASTA: {}", path.display()))
}

/// Parse all FASTA records from a string.
pub fn parse_fasta_string(input: &str) -> Result<Vec<FastaRecord>> {
    parse_fasta_reader(input.as_bytes())
}

/// Write records to a writer in FASTA format.
pub fn write_fasta<W: io::Write>(writer: &mut W, records: &[FastaRecord]) -> Result<()> {
    for rec in records {
        write!(writer, ">{}\n", rec.header)?;
        for chunk in rec.seq.chunks(80) {
            let s = std::str::from_utf8(chunk).unwrap_or("?");
            writeln!(writer, "{}", s)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_record() {
        let input = ">seq1 test sequence\nACGTACGT\n";
        let records = parse_fasta_string(input).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].header, "seq1 test sequence");
        assert_eq!(records[0].seq, b"ACGTACGT");
        assert_eq!(records[0].id(), "seq1");
    }

    #[test]
    fn test_parse_multi_record() {
        let input = ">seq1\nACGT\n>seq2\nTTTT\n>seq3\nCCCCGGGG\n";
        let records = parse_fasta_string(input).unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].seq, b"ACGT");
        assert_eq!(records[1].seq, b"TTTT");
        assert_eq!(records[2].seq, b"CCCCGGGG");
    }

    #[test]
    fn test_parse_multiline_sequence() {
        let input = ">seq1\nACGT\nTGCA\nAAAA\n";
        let records = parse_fasta_string(input).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].seq, b"ACGTTGCAAAAA");
    }

    #[test]
    fn test_parse_lowercase_to_uppercase() {
        let input = ">seq1\nacgt\n";
        let records = parse_fasta_string(input).unwrap();
        assert_eq!(records[0].seq, b"ACGT");
    }

    #[test]
    fn test_parse_empty_input() {
        let records = parse_fasta_string("").unwrap();
        assert!(records.is_empty());
    }

    #[test]
    fn test_parse_whitespace_handling() {
        let input = ">seq1\nA C G T\nT G C A\n";
        let records = parse_fasta_string(input).unwrap();
        assert_eq!(records[0].seq, b"ACGTTGCA");
    }

    #[test]
    fn test_fasta_record_display() {
        let rec = FastaRecord {
            header: "test".to_string(),
            seq: b"ACGTACGTACGTACGTACGT".to_vec(),
        };
        let display = format!("{}", rec);
        assert!(display.starts_with(">test\n"));
    }

    #[test]
    fn test_write_and_read_roundtrip() {
        let records = vec![
            FastaRecord {
                header: "seq1".to_string(),
                seq: b"ACGTACGT".to_vec(),
            },
            FastaRecord {
                header: "seq2".to_string(),
                seq: b"TTTTCCCC".to_vec(),
            },
        ];
        let mut buf = Vec::new();
        write_fasta(&mut buf, &records).unwrap();
        let parsed = parse_fasta_string(&String::from_utf8(buf).unwrap()).unwrap();
        assert_eq!(records, parsed);
    }

    #[test]
    fn test_empty_record() {
        let input = ">empty\n";
        let records = parse_fasta_string(input).unwrap();
        assert_eq!(records.len(), 1);
        assert!(records[0].is_empty());
    }

    #[test]
    fn test_dna_ambiguity_codes() {
        let input = ">seq1\nACGTNRYSWKMBDHV\n";
        let records = parse_fasta_string(input).unwrap();
        assert_eq!(records[0].seq.len(), 15);
    }

    #[test]
    fn test_protein_sequences() {
        let input = ">prot1\nMKTAYIAKQRQISFVKSHFSRQDILDLWIYHTQGYFP\n";
        let records = parse_fasta_string(input).unwrap();
        assert_eq!(records.len(), 1);
        assert!(records[0].seq.len() > 0);
    }
}
