//! FASTA format parser — streaming, multi-line aware, optional gzip.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use flate2::read::GzDecoder;

use crate::error::BioError;

/// A single FASTA record.
#[derive(Debug, Clone, PartialEq)]
pub struct FastaRecord {
    /// Identifier (first whitespace-delimited token after `>`)
    pub id: String,
    /// Description (rest of the header line after the id)
    pub description: String,
    /// Concatenated sequence lines (all uppercase, no whitespace)
    pub sequence: String,
}

impl FastaRecord {
    /// GC content as a fraction of total bases (0.0–1.0).
    /// Returns 0.0 for empty sequences.
    pub fn gc_content(&self) -> f64 {
        if self.sequence.is_empty() {
            return 0.0;
        }
        let gc = self.sequence.chars().filter(|c| *c == 'G' || *c == 'C').count();
        gc as f64 / self.sequence.len() as f64
    }

    /// Sequence length.
    pub fn len(&self) -> usize {
        self.sequence.len()
    }

    /// Whether the sequence is empty.
    pub fn is_empty(&self) -> bool {
        self.sequence.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/// Stateful streaming parser over any `BufRead` source.
pub struct FastaReader<R: BufRead> {
    reader: R,
    buf: String,
    line_no: usize,
    /// Buffered next header line (when we've read ahead past a record).
    next_header: Option<String>,
    done: bool,
}

impl<R: BufRead> FastaReader<R> {
    pub fn new(reader: R) -> Self {
        FastaReader {
            reader,
            buf: String::new(),
            line_no: 0,
            next_header: None,
            done: false,
        }
    }

    /// Read the next record. Returns `Ok(None)` at EOF.
    pub fn next_record(&mut self) -> Result<Option<FastaRecord>, BioError> {
        if self.done {
            return Ok(None);
        }

        // --- find header line ---
        let header = if let Some(h) = self.next_header.take() {
            h
        } else {
            loop {
                self.buf.clear();
                let n = self.reader.read_line(&mut self.buf)?;
                self.line_no += 1;
                if n == 0 {
                    self.done = true;
                    return Ok(None);
                }
                let trimmed = self.buf.trim();
                if trimmed.starts_with('>') {
                    break trimmed.to_string();
                }
                // skip blank / non-header lines before first record
                if !trimmed.is_empty() {
                    return Err(BioError::Parse {
                        message: format!("Expected '>' header, got: '{}'", trimmed),
                        line: Some(self.line_no),
                    });
                }
            }
        };

        // --- parse header ---
        let header_inner = &header[1..]; // strip '>'
        let (id, description) = match header_inner.find(char::is_whitespace) {
            Some(pos) => (header_inner[..pos].to_string(), header_inner[pos..].trim().to_string()),
            None => (header_inner.to_string(), String::new()),
        };

        // --- accumulate sequence lines until next header or EOF ---
        let mut sequence = String::new();
        loop {
            self.buf.clear();
            let n = self.reader.read_line(&mut self.buf)?;
            self.line_no += 1;
            if n == 0 {
                self.done = true;
                break;
            }
            let trimmed = self.buf.trim();
            if trimmed.starts_with('>') {
                self.next_header = Some(trimmed.to_string());
                break;
            }
            if !trimmed.is_empty() {
                sequence.push_str(trimmed);
            }
        }

        // Uppercase the sequence and strip any remaining whitespace
        let sequence: String = sequence.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase();

        Ok(Some(FastaRecord { id, description, sequence }))
    }
}

/// Iterate over records lazily.
pub struct FastaIterator<R: BufRead> {
    reader: FastaReader<R>,
}

impl<R: BufRead> Iterator for FastaIterator<R> {
    type Item = Result<FastaRecord, BioError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.reader.next_record().transpose()
    }
}

// ---------------------------------------------------------------------------
// Public constructors
// ---------------------------------------------------------------------------

/// Parse FASTA from any `Read` source.
pub fn parse_reader<R: Read>(reader: R) -> FastaIterator<BufReader<R>> {
    FastaIterator { reader: FastaReader::new(BufReader::new(reader)) }
}

/// Parse a FASTA file (auto-detects `.gz` by extension).
pub fn parse_file(path: &str) -> Result<FastaIterator<BufReader<Box<dyn Read>>>, BioError> {
    let file = File::open(path)?;
    let reader: Box<dyn Read> = if path.ends_with(".gz") {
        Box::new(GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    Ok(FastaIterator { reader: FastaReader::new(BufReader::new(reader)) })
}

/// Parse FASTA from stdin.
pub fn parse_stdin() -> FastaIterator<io::StdinLock<'static>> {
    let stdin = io::stdin();
    FastaIterator { reader: FastaReader::new(stdin.lock()) }
}

/// Convenience: collect all records into a Vec.
pub fn parse_to_vec(path: &str) -> Result<Vec<FastaRecord>, BioError> {
    parse_file(path)?.collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_FASTA: &str = ">seq1 some description\nACGT\nACGT\n>seq2\nTTTT\n";

    const EMPTY_FILE: &str = "";

    const SINGLE_RECORD: &str = ">only\nACGTN\n";

    const WRAPPED_LINES: &str = ">wrap\nACGT\nTGCA\nAAAA\nGGGG\n";

    const NO_DESCRIPTION: &str = ">id\nAC\n";

    fn parse_str(s: &str) -> Vec<FastaRecord> {
        parse_reader(s.as_bytes()).collect::<Result<Vec<_>, _>>().unwrap()
    }

    #[test]
    fn test_simple_parse() {
        let recs = parse_str(SIMPLE_FASTA);
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].id, "seq1");
        assert_eq!(recs[0].description, "some description");
        assert_eq!(recs[0].sequence, "ACGTACGT");
        assert_eq!(recs[1].id, "seq2");
        assert_eq!(recs[1].sequence, "TTTT");
    }

    #[test]
    fn test_empty_file() {
        let recs = parse_str(EMPTY_FILE);
        assert!(recs.is_empty());
    }

    #[test]
    fn test_single_record() {
        let recs = parse_str(SINGLE_RECORD);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, "only");
        assert_eq!(recs[0].sequence, "ACGTN");
    }

    #[test]
    fn test_wrapped_lines() {
        let recs = parse_str(WRAPPED_LINES);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].sequence, "ACGTTGCAA AAAGGGG".replace(' ', ""));
    }

    #[test]
    fn test_no_description() {
        let recs = parse_str(NO_DESCRIPTION);
        assert_eq!(recs[0].id, "id");
        assert!(recs[0].description.is_empty());
    }

    #[test]
    fn test_gc_content() {
        let rec = FastaRecord {
            id: "test".into(),
            description: String::new(),
            sequence: "ACGT".into(),
        };
        assert!((rec.gc_content() - 0.5).abs() < 1e-10);

        let empty = FastaRecord {
            id: "e".into(),
            description: String::new(),
            sequence: String::new(),
        };
        assert!((empty.gc_content()).abs() < 1e-10);
    }

    #[test]
    fn test_lowercase_input() {
        let input = ">lc\nacgt\n";
        let recs = parse_str(input);
        assert_eq!(recs[0].sequence, "ACGT");
    }
}
