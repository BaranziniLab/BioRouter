# bio-fasta-fastq-toolkit-rs

A streaming FASTA/FASTQ bioinformatics toolkit library and CLI, written in Rust.

## Features

- **Streaming parsers** for FASTA and FASTQ formats (multi-line records, gzipped input)
- **Sequence statistics**: length distribution, GC content, N50/L50, base composition
- **FASTQ quality analysis**: per-base mean quality, Phred score decoding (Sanger/Illumina), quality filtering/trimming with sliding window
- **Format conversion**: FASTQ → FASTA
- **Subsampling**: random subsampling of records
- **Sequence operations**: reverse complement, DNA→protein translation
- **CLI** with subcommands: `stats`, `filter`, `trim`, `convert`, `subsample`

## Usage

```bash
# Sequence statistics
cargo run -- stats input.fasta
cargo run -- stats --format fastq input.fastq.gz

# Quality filtering
cargo run -- filter --min-qual 20 input.fastq

# Sliding-window quality trimming
cargo run -- trim --window-size 5 --min-qual 20 input.fastq

# Format conversion (FASTQ → FASTA)
cargo run -- convert input.fastq

# Random subsampling (10% of records)
cargo run -- subsample --fraction 0.1 input.fastq

# Read from stdin
cat input.fasta | cargo run -- stats --format fasta -
```

## Library

```rust
use bio_fasta_fastq_toolkit::fasta;
use bio_fasta_fastq_toolkit::fastq;
use bio_fasta_fastq_toolkit::stats;

let records: Vec<_> = fasta::parse_file("genome.fasta").unwrap().collect();
let composition = stats::base_composition(&records[0].sequence);
```

## Build & Test

```bash
cargo build
cargo test
```

## License

MIT
