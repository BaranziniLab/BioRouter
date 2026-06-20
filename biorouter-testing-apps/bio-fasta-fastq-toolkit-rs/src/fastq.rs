//! FASTQ format parser — streaming, gzip-aware, strict length-mismatch checks.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use flate2::read::GzDecoder;

use crate::error::BioError;

/// A single FASTQ record.
#[derive(Debug, Clone, PartialEq)]
pub struct FastqRecord {
    /// Identifier (first whitespace-delimited token of header line, without '@')
    pub id: String,
    /// Rest of header after id.
    pub description: String,
    /// Raw sequence string (uppercase, no whitespace).
    pub sequence: String,
    /// Quality string (ASCII, same length as sequence).
    pub quality: String,
}

impl FastqRecord {
    /// GC content of the sequence (0.0–1.0).
    pub fn gc_content(&self) -> f64 {
        if self.sequence.is_empty() {
            return 0.0;
        }
        let gc = self.sequence.chars().filter(|c| *c == 'G' || *c == 'C').count();
        gc as f64 / self.sequence.len() as f64
    }

    pub fn len(&self) -> usize {
        self.sequence.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sequence.is_empty()
    }

    /// Convert to a FastaRecord (drops quality).
    pub fn to_fasta(&self) -> crate::fasta::FastaRecord {
        crate::fasta::FastaRecord {
            id: self.id.clone(),
            description: self.description.clone(),
            sequence: self.sequence.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Streaming parser
// ---------------------------------------------------------------------------

/// Stateful streaming FASTQ parser over a `BufRead`.
pub struct FastqReader<R: BufRead> {
    reader: R,
    buf: String,
    line_no: usize,
}

impl<R: BufRead> FastqReader<R> {
    pub fn new(reader: R) -> Self {
        FastqReader { reader, buf: String::new(), line_no: 0 }
    }

    /// Read the next FASTQ record (4 lines). Returns `Ok(None)` at EOF.
    pub fn next_record(&mut self) -> Result<Option<FastqRecord>, BioError> {
        // --- 1. header ---
        loop {
            self.buf.clear();
            let n = self.reader.read_line(&mut self.buf)?;
            self.line_no += 1;
            if n == 0 {
                return Ok(None); // EOF
            }
            let trimmed = self.buf.trim();
            if !trimmed.is_empty() {
                if !trimmed.starts_with('@') {
                    return Err(BioError::Parse {
                        message: format!("Expected '@' header, got: '{}'", trimmed),
                        line: Some(self.line_no),
                    });
                }
                let header_inner = &trimmed[1..];
                let (id, description) = match header_inner.find(char::is_whitespace) {
                    Some(pos) => (
                        header_inner[..pos].to_string(),
                        header_inner[pos..].trim().to_string(),
                    ),
                    None => (header_inner.to_string(), String::new()),
                };
                // --- 2. sequence ---
                self.buf.clear();
                let n = self.reader.read_line(&mut self.buf)?;
                self.line_no += 1;
                if n == 0 {
                    return Err(BioError::Parse {
                        message: "Unexpected EOF after header".into(),
                        line: Some(self.line_no),
                    });
                }
                let sequence: String =
                    self.buf.trim().chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase();

                // --- 3. '+' separator ---
                self.buf.clear();
                let n = self.reader.read_line(&mut self.buf)?;
                self.line_no += 1;
                if n == 0 {
                    return Err(BioError::Parse {
                        message: "Unexpected EOF, expected '+' line".into(),
                        line: Some(self.line_no),
                    });
                }
                let sep = self.buf.trim();
                if !sep.starts_with('+') {
                    return Err(BioError::Parse {
                        message: format!("Expected '+' separator, got: '{}'", sep),
                        line: Some(self.line_no),
                    });
                }

                // --- 4. quality ---
                self.buf.clear();
                let n = self.reader.read_line(&mut self.buf)?;
                self.line_no += 1;
                if n == 0 {
                    return Err(BioError::Parse {
                        message: "Unexpected EOF after '+' line".into(),
                        line: Some(self.line_no),
                    });
                }
                let quality: String =
                    self.buf.trim().chars().filter(|c| !c.is_whitespace()).collect();

                // --- length check ---
                if sequence.len() != quality.len() {
                    return Err(BioError::LengthMismatch {
                        seq_len: sequence.len(),
                        qual_len: quality.len(),
                        record_id: id,
                    });
                }

                return Ok(Some(FastqRecord { id, description, sequence, quality }));
            }
            // skip blank lines between records
        }
    }
}

/// Iterator wrapper for `FastqReader`.
pub struct FastqIterator<R: BufRead> {
    reader: FastqReader<R>,
}

impl<R: BufRead> Iterator for FastqIterator<R> {
    type Item = Result<FastqRecord, BioError>;
    fn next(&mut self) -> Option<Self::Item> {
        self.reader.next_record().transpose()
    }
}

// ---------------------------------------------------------------------------
// Public constructors
// ---------------------------------------------------------------------------

pub fn parse_reader<R: Read>(reader: R) -> FastqIterator<BufReader<R>> {
    FastqIterator { reader: FastqReader::new(BufReader::new(reader)) }
}

pub fn parse_file(path: &str) -> Result<FastqIterator<BufReader<Box<dyn Read>>>, BioError> {
    let file = File::open(path)?;
    let reader: Box<dyn Read> = if path.ends_with(".gz") {
        Box::new(GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    Ok(FastqIterator { reader: FastqReader::new(BufReader::new(reader)) })
}

pub fn parse_stdin() -> FastqIterator<io::StdinLock<'static>> {
    let stdin = io::stdin();
    FastqIterator { reader: FastqReader::new(stdin.lock()) }
}

pub fn parse_to_vec(path: &str) -> Result<Vec<FastqRecord>, BioError> {
    parse_file(path)?.collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_FASTQ: &str = "@read1 desc\nACGT\n+\nIIII\n@read2\nTTTT\n+\n!!!!\n";

    const EMPTY_FILE: &str = "";

    fn parse_str(s: &str) -> Vec<FastqRecord> {
        parse_reader(s.as_bytes()).collect::<Result<Vec<_>, _>>().unwrap()
    }

    #[test]
    fn test_simple_parse() {
        let recs = parse_str(SIMPLE_FASTQ);
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].id, "read1");
        assert_eq!(recs[0].description, "desc");
        assert_eq!(recs[0].sequence, "ACGT");
        assert_eq!(recs[0].quality, "IIII");
        assert_eq!(recs[1].id, "read2");
        assert_eq!(recs[1].sequence, "TTTT");
    }

    #[test]
    fn test_empty_file() {
        let recs = parse_str(EMPTY_FILE);
        assert!(recs.is_empty());
    }

    #[test]
    fn test_single_record() {
        let input = "@solo\nACGTN\n+\n!!!!!\n";
        let recs = parse_str(input);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, "solo");
    }

    #[test]
    fn test_length_mismatch() {
        // Sequence is 4 bases, quality is 3 characters.
        let input = "@bad\nACGT\n+\nIII\n";
        let result: Result<Vec<_>, _> = parse_reader(input.as_bytes()).collect();
        assert!(result.is_err());
        match result.unwrap_err() {
            BioError::LengthMismatch { .. } => {}
            other => panic!("Expected LengthMismatch, got: {}", other),
        }
    }

    #[test]
    fn test_lowercase_sequence() {
        let input = "@lc\nacgt\n+\nIIII\n";
        let recs = parse_str(input);
        assert_eq!(recs[0].sequence, "ACGT");
    }

    #[test]
    fn test_gc_content() {
        let recs = parse_str(SIMPLE_FASTQ);
        assert!((recs[0].gc_content() - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_to_fasta() {
        let recs = parse_str(SIMPLE_FASTQ);
        let fasta = recs[0].to_fasta();
        assert_eq!(fasta.id, "read1");
        assert_eq!(fasta.description, "desc");
        assert_eq!(fasta.sequence, "ACGT");
    }
}
