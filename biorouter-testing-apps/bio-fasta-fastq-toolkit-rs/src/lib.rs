//! bio-fasta-fastq-toolkit — a streaming FASTA/FASTQ bioinformatics toolkit.
//!
//! Provides parsers, sequence statistics, quality analysis, format conversion,
//! and sequence operations (reverse complement, translation, subsampling).

pub mod error;
pub mod fasta;
pub mod fastq;
pub mod stats;
pub mod quality;
pub mod convert;
pub mod seqops;
pub mod cli;
