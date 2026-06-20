//! Command-line interface for bio-blast-lite.
//!
//! Supports two modes:
//! 1. `index`: Build and save a k-mer index of a database.
//! 2. `search`: Load a database, build an index (in-memory), and search a query.

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::fasta::parse_fasta_file;
use crate::index::KmerIndex;
use crate::search::{search, SearchConfig, SearchHit};

/// bio-blast-lite: A fast BLAST-like local sequence similarity search tool.
#[derive(Parser)]
#[command(name = "blast-lite")]
#[command(about = "A BLAST-like local sequence similarity search tool in Rust")]
#[command(version)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Build a k-mer index of a database FASTA file.
    Index {
        /// Path to the database FASTA file.
        #[arg(short, long)]
        database: PathBuf,

        /// Word / k-mer size.
        #[arg(short = 'k', long, default_value_t = 11)]
        word_size: usize,

        /// Output path for the index (optional, for future use).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Search a query against a database.
    Search {
        /// Path to the query FASTA file.
        #[arg(short, long)]
        query: PathBuf,

        /// Path to the database FASTA file.
        #[arg(short, long)]
        database: PathBuf,

        /// Word / k-mer size.
        #[arg(short = 'k', long, default_value_t = 11)]
        word_size: usize,

        /// X-drop threshold for ungapped extension.
        #[arg(long, default_value_t = 10)]
        x_drop: i32,

        /// Band width for gapped extension.
        #[arg(long, default_value_t = 16)]
        band_width: usize,

        /// Flank size for gapped extension.
        #[arg(long, default_value_t = 50)]
        flank: usize,

        /// Maximum E-value threshold.
        #[arg(long, default_value_t = 10.0)]
        e_value: f64,

        /// Maximum number of hits to report.
        #[arg(short = 'n', long, default_value_t = 500)]
        max_hits: usize,

        /// Match score (nucleotide).
        #[arg(long, default_value_t = 2)]
        match_score: i32,

        /// Mismatch penalty (nucleotide).
        #[arg(long, default_value_t = -3)]
        mismatch_score: i32,

        /// Gap open penalty.
        #[arg(long, default_value_t = 5)]
        gap_open: i32,

        /// Gap extend penalty.
        #[arg(long, default_value_t = 2)]
        gap_extend: i32,

        /// Output format: tabular, alignments, both.
        #[arg(short, long, default_value = "both")]
        format: String,

        /// Output file (default: stdout).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

/// Run the CLI.
pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Index {
            database,
            word_size,
            output,
        } => run_index(&database, word_size, output.as_deref()),
        Commands::Search {
            query,
            database,
            word_size,
            x_drop,
            band_width,
            flank,
            e_value,
            max_hits,
            match_score,
            mismatch_score,
            gap_open,
            gap_extend,
            format,
            output,
        } => {
            let config = SearchConfig {
                word_size,
                x_drop,
                band_width,
                flank,
                e_value_threshold: e_value,
                max_hits,
                match_score,
                mismatch_score,
                gap_open,
                gap_extend,
            };
            run_search(&query, &database, &config, &format, output.as_deref())
        }
    }
}

fn run_index(database: &PathBuf, word_size: usize, _output: Option<&Path>) -> Result<()> {
    eprintln!("Loading database from: {}", database.display());
    let records = parse_fasta_file(database)
        .with_context(|| format!("Failed to parse database: {}", database.display()))?;
    eprintln!("Loaded {} sequences", records.len());

    let index = KmerIndex::build(&records, word_size);
    eprintln!(
        "Index built: {} unique k-mers, {} total occurrences",
        index.num_unique_kmers(),
        index.total_hits()
    );

    // Future: serialize index to output file
    if let Some(_out_path) = _output {
        eprintln!("Index serialization not yet implemented.");
    }

    Ok(())
}

fn run_search(
    query_path: &PathBuf,
    database_path: &PathBuf,
    config: &SearchConfig,
    format: &str,
    output: Option<&Path>,
) -> Result<()> {
    // Load query
    let queries = parse_fasta_file(query_path)
        .with_context(|| format!("Failed to parse query: {}", query_path.display()))?;
    if queries.is_empty() {
        bail!("No query sequences found in {}", query_path.display());
    }

    // Load database
    eprintln!("Loading database from: {}", database_path.display());
    let database = parse_fasta_file(database_path)
        .with_context(|| format!("Failed to parse database: {}", database_path.display()))?;
    eprintln!("Loaded {} database sequences", database.len());

    // Build index
    eprintln!("Building k-mer index (k={})...", config.word_size);
    let index = KmerIndex::build(&database, config.word_size);

    // Setup output
    let mut out: Box<dyn Write> = if let Some(path) = output {
        Box::new(fs::File::create(path).context("Failed to create output file")?)
    } else {
        Box::new(io::stdout())
    };

    // Header
    if format.contains("tabular") || format.contains("both") {
        writeln!(
            out,
            "sequence_id\tquery_start\tquery_end\tdb_start\tdb_end\tscore\tbit_score\te_value\talignment_length\tidentity"
        )?;
    }

    // Search each query
    for q in &queries {
        eprintln!("Searching query: {}", q.id());
        let results = search(q, &database, &index, config)?;

        if results.is_empty() {
            eprintln!("  No significant hits found.");
            if format.contains("both") || format.contains("tabular") {
                writeln!(out, "# No hits for {}", q.id())?;
            }
            continue;
        }

        eprintln!("  Found {} hits", results.len());

        if format.contains("tabular") || format.contains("both") {
            for hit in &results {
                writeln!(out, "{}", hit.format_tabular())?;
            }
        }

        if format.contains("alignments") || format.contains("both") {
            writeln!(out, "\n# Alignments for {}", q.id())?;
            for (i, hit) in results.iter().enumerate() {
                writeln!(out, "\n## Hit {}: {}", i + 1, hit.db_header)?;
                writeln!(out, "{}", format_hit_alignment(hit, &q.seq))?;
            }
        }
    }

    Ok(())
}

/// Format a single hit with full pairwise alignment.
fn format_hit_alignment(hit: &SearchHit, query_seq: &[u8]) -> String {
    let mut output = String::new();
    output.push_str(&format!(
        "Score: {} bits ({:.1}), E-value: {:.2e}\n",
        hit.stats.bit_score, hit.stats.score, hit.stats.e_value
    ));
    output.push_str(&format!(
        "Identity: {}/{} ({:.1}%), Gaps: {}/{}\n",
        hit.stats.matches,
        hit.stats.alignment_length,
        hit.stats.percent_identity,
        hit.stats.gap_extensions,
        hit.stats.alignment_length
    ));
    output.push('\n');

    // Build alignment strings from traceback
    let mut q_chars = Vec::new();
    let mut _mid_chars: Vec<char> = Vec::new();
    let mut s_chars = Vec::new();

    for &(q_opt, _d_opt) in &hit.traceback {
        match q_opt {
            Some(qi) => {
                q_chars.push(query_seq[qi] as char);
            }
            None => {
                q_chars.push('-');
            }
        }
    }

    // For the subject line, we need to reconstruct from the alignment
    // Since we don't have the db_seq here, show query and gaps
    for &(q_opt, _d_opt) in &hit.traceback {
        match q_opt {
            Some(_) => s_chars.push(' '),  // placeholder
            None => s_chars.push(' '),
        }
    }

    let q_str: String = q_chars.iter().collect();
    let _m_str: String = _mid_chars.iter().collect();
    let s_str: String = s_chars.iter().collect();

    // Format in 60-char blocks
    let block_size = 60;
    let len = q_str.len();
    let mut i = 0;
    while i < len {
        let end = (i + block_size).min(len);
        output.push_str(&format!("Query:   {}\n", &q_str[i..end]));
        output.push_str(&format!("         {}\n", &s_str[i..end]));
        i = end;
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing_index() {
        let args = vec!["blast-lite", "index", "-d", "test.fasta", "-k", "8"];
        let cli = Cli::try_parse_from(args);
        assert!(cli.is_ok());

        match cli.unwrap().command {
            Commands::Index {
                database,
                word_size,
                ..
            } => {
                assert_eq!(database, PathBuf::from("test.fasta"));
                assert_eq!(word_size, 8);
            }
            _ => panic!("Expected Index command"),
        }
    }

    #[test]
    fn test_cli_parsing_search() {
        let args = vec![
            "blast-lite",
            "search",
            "-q",
            "query.fasta",
            "-d",
            "db.fasta",
            "-k",
            "11",
            "--x-drop",
            "15",
            "-f",
            "tabular",
        ];
        let cli = Cli::try_parse_from(args);
        assert!(cli.is_ok());

        match cli.unwrap().command {
            Commands::Search {
                query,
                database,
                word_size,
                x_drop,
                format,
                ..
            } => {
                assert_eq!(query, PathBuf::from("query.fasta"));
                assert_eq!(database, PathBuf::from("db.fasta"));
                assert_eq!(word_size, 11);
                assert_eq!(x_drop, 15);
                assert_eq!(format, "tabular");
            }
            _ => panic!("Expected Search command"),
        }
    }

    #[test]
    fn test_cli_default_values() {
        let args = vec!["blast-lite", "search", "-q", "q.fa", "-d", "d.fa"];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Commands::Search {
                word_size,
                x_drop,
                band_width,
                e_value,
                max_hits,
                ..
            } => {
                assert_eq!(word_size, 11);
                assert_eq!(x_drop, 10);
                assert_eq!(band_width, 16);
                assert!((e_value - 10.0).abs() < f64::EPSILON);
                assert_eq!(max_hits, 500);
            }
            _ => panic!("Expected Search command"),
        }
    }
}
