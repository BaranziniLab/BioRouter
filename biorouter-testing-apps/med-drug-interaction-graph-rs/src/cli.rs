use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Drug-Drug Interaction Graph Engine CLI
#[derive(Parser, Debug)]
#[command(name = "ddi-graph", about = "A drug-drug interaction graph engine")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Load a drug database and query interactions
    Query {
        /// Path to the drug database (JSON format)
        #[arg(short, long)]
        database: PathBuf,

        /// Comma-separated list of medications
        #[arg(short, long)]
        medications: String,

        /// Maximum chain length to detect
        #[arg(short = 'c', long, default_value = "4")]
        max_chain: usize,

        /// Show detailed mechanisms
        #[arg(long)]
        detailed: bool,
    },

    /// Show interactions for a specific drug
    Drug {
        /// Path to the drug database (JSON format)
        #[arg(short, long)]
        database: PathBuf,

        /// Drug name to search
        #[arg(short, long)]
        name: String,

        /// Show all drugs in the database
        #[arg(long)]
        list_all: bool,
    },

    /// Find alternative medications
    Alternatives {
        /// Path to the drug database (JSON format)
        #[arg(short, long)]
        database: PathBuf,

        /// Drug to find alternatives for
        #[arg(short, long)]
        for_drug: String,

        /// Current medication regimen (comma-separated)
        #[arg(short, long)]
        regimen: String,

        /// Include alternatives from different drug classes
        #[arg(long)]
        broad: bool,
    },

    /// Analyze graph centrality and find hub drugs
    Analyze {
        /// Path to the drug database (JSON format)
        #[arg(short, long)]
        database: PathBuf,

        /// Show connected components
        #[arg(long)]
        components: bool,

        /// Show centrality rankings
        #[arg(long)]
        centrality: bool,

        /// Find hub drugs (above given percentile, 0.0-1.0)
        #[arg(long)]
        hubs: Option<f64>,
    },

    /// Compare two drug regimens for safety
    Compare {
        /// Path to the drug database (JSON format)
        #[arg(short, long)]
        database: PathBuf,

        /// First regimen (comma-separated)
        #[arg(short = 'a', long)]
        regimen_a: String,

        /// Second regimen (comma-separated)
        #[arg(short = 'b', long)]
        regimen_b: String,
    },
}
