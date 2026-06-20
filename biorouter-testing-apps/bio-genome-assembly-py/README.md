# bio-genome-assembly-py

A mini de-novo genome assembler written in pure Python.

## Features

- **Two Assembly Algorithms**:
  - **Overlap-Layout-Consensus (OLC)**: Best for long reads (PacBio, Nanopore)
  - **De Bruijn Graph (DBG)**: Best for short reads (Illumina)

- **Read Simulation**: Generate simulated reads from reference sequences for testing

- **Assembly Metrics**: N50, L50, GC content, contig statistics

- **Command-Line Interface**: Easy-to-use CLI for assembly, simulation, and statistics

## Installation

```bash
# Clone the repository
git clone <repository-url>
cd bio-genome-assembly-py

# Install in development mode
pip install -e .
```

## Usage

### Assemble Reads

```bash
# Using de Bruijn graph (default)
bioassembly assemble -i reads.fastq -o contigs.fasta

# Using OLC algorithm
bioassembly assemble -i reads.fasta -o contigs.fasta --method olc

# With custom k-mer size
bioassembly assemble -i reads.fastq -o contigs.fasta -k 31
```

### Simulate Reads

```bash
# Simulate Illumina-like reads
bioassembly simulate -r reference.fasta -o reads.fastq -n 10000

# With custom error rate
bioassembly simulate -r reference.fasta -o reads.fastq --error-rate 0.01
```

### Compute Statistics

```bash
# Print assembly statistics
bioassembly stats -i contigs.fasta

# Save to file
bioassembly stats -i contigs.fasta -o stats.txt
```

## Python API

```python
from bio_assembly.io import read_sequences, write_fasta
from bio_assembly.dbg import assemble_dbg
from bio_assembly.olc import assemble_olc
from bio_assembly.simulate import simulate_short_reads

# Read input
reads = read_sequences("reads.fastq")

# Assemble with DBG
contigs, stats = assemble_dbg(reads, k=21)

# Or assemble with OLC
contigs, stats = assemble_olc(reads, min_overlap=500)

# Write output
write_fasta(contigs, "contigs.fasta")
print(stats.summary())
```

## Project Structure

```
bio-genome-assembly-py/
├── src/
│   └── bio_assembly/
│       ├── __init__.py      # Package initialization
│       ├── io.py            # FASTA/FASTQ I/O
│       ├── overlap.py       # Overlap detection
│       ├── olc.py           # OLC assembler
│       ├── dbg.py           # De Bruijn graph assembler
│       ├── consensus.py     # Consensus generation
│       ├── metrics.py       # Assembly metrics
│       ├── simulate.py      # Read simulator
│       └── cli.py           # Command-line interface
├── tests/
│   ├── test_io.py           # I/O tests
│   ├── test_overlap.py      # Overlap tests
│   ├── test_metrics.py      # Metrics tests
│   ├── test_dbg.py          # DBG tests
│   └── test_assembly.py     # Integration tests
├── pyproject.toml           # Project configuration
└── README.md                # This file
```

## Algorithm Details

### Overlap-Layout-Consensus (OLC)

1. **Overlap**: Compute pairwise suffix-prefix overlaps between reads
2. **Layout**: Build overlap graph and find assembly paths
3. **Consensus**: Generate consensus sequences from aligned reads

### De Bruijn Graph (DBG)

1. **Build**: Extract k-mers from reads and build graph
2. **Simplify**: Remove tips, bubbles, and low-coverage nodes
3. **Extract**: Collapse unitigs into contigs

## Testing

```bash
# Run all tests
pytest

# Run with verbose output
pytest -v

# Run specific test file
pytest tests/test_assembly.py
```

## License

MIT License
