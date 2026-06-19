//! CLI argument parsing using clap.

use clap::{Parser, Subcommand};

/// A streaming FASTA/FASTQ bioinformatics toolkit.
#[derive(Parser, Debug)]
#[command(name = "bio-toolkit", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Display sequence statistics (length distribution, GC, N50, base composition).
    Stats {
        /// Input file path (or '-' for stdin).
        input: String,
        /// Input format: fasta or fastq.
        #[arg(short, long, default_value = "fasta")]
        format: String,
    },
    /// Filter FASTQ records by minimum mean quality.
    Filter {
        /// Input FASTQ file (or '-' for stdin).
        input: String,
        /// Minimum mean quality (Phred score).
        #[arg(short = 'q', long)]
        min_qual: f64,
        /// Quality encoding: sanger or illumina.
        #[arg(short, long, default_value = "sanger")]
        encoding: String,
        /// Output file (default: stdout).
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Trim FASTQ records using sliding-window quality trimming.
    Trim {
        /// Input FASTQ file (or '-' for stdin).
        input: String,
        /// Sliding window size.
        #[arg(short, long, default_value_t = 4)]
        window_size: usize,
        /// Minimum mean quality within the window.
        #[arg(short = 'q', long)]
        min_qual: f64,
        /// Quality encoding: sanger or illumina.
        #[arg(short, long, default_value = "sanger")]
        encoding: String,
        /// Output file (default: stdout).
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Convert FASTQ to FASTA.
    Convert {
        /// Input FASTQ file (or '-' for stdin).
        input: String,
        /// Output file (default: stdout).
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Randomly subsample records.
    Subsample {
        /// Input file (or '-' for stdin).
        input: String,
        /// Fraction of records to keep (0.0–1.0).
        #[arg(short, long)]
        fraction: f64,
        /// Input format: fasta or fastq.
        #[arg(short, long, default_value = "fasta")]
        format: String,
    },
    /// Reverse complement sequences.
    Revcomp {
        /// Input FASTA file (or '-' for stdin).
        input: String,
    },
    /// Translate DNA sequences to protein.
    Translate {
        /// Input FASTA file (or '-' for stdin).
        input: String,
    },
}
