//! Error types for the bio-fasta-fastq-toolkit.

use std::fmt;
use std::io;

/// All errors that can occur in this toolkit.
#[derive(Debug)]
pub enum BioError {
    /// An I/O error (file not found, read failure, etc.)
    Io(io::Error),
    /// A malformed record was encountered during parsing.
    Parse { message: String, line: Option<usize> },
    /// An invalid sequence character was found.
    InvalidSequence { char: char, position: usize },
    /// Quality string length does not match sequence length.
    LengthMismatch { seq_len: usize, qual_len: usize, record_id: String },
    /// Unsupported or unrecognized format.
    UnsupportedFormat(String),
    /// Invalid quality encoding.
    InvalidQualityEncoding(String),
}

impl fmt::Display for BioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BioError::Io(e) => write!(f, "I/O error: {}", e),
            BioError::Parse { message, line } => {
                if let Some(l) = line {
                    write!(f, "Parse error at line {}: {}", l, message)
                } else {
                    write!(f, "Parse error: {}", message)
                }
            }
            BioError::InvalidSequence { char, position } => {
                write!(f, "Invalid sequence character '{}' at position {}", char, position)
            }
            BioError::LengthMismatch { seq_len, qual_len, record_id } => {
                write!(
                    f,
                    "Quality length ({}) does not match sequence length ({}) for record '{}'",
                    qual_len, seq_len, record_id
                )
            }
            BioError::UnsupportedFormat(msg) => write!(f, "Unsupported format: {}", msg),
            BioError::InvalidQualityEncoding(msg) => write!(f, "Invalid quality encoding: {}", msg),
        }
    }
}

impl std::error::Error for BioError {}

impl From<io::Error> for BioError {
    fn from(e: io::Error) -> Self {
        BioError::Io(e)
    }
}
