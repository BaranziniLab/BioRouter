# Bio-Motif-Finder-Py

A DNA motif-discovery toolkit implementing multiple algorithms for finding regulatory motifs in DNA sequences.

## Features

- **Multiple Algorithms**: Greedy median-string, Gibbs sampling, and EM-style (MEME-lite)
- **Position Weight Matrix (PWM)**: Full PWM utilities with log-odds scoring
- **Information Content**: Relative entropy scoring against background model
- **Consensus Extraction**: Automatic consensus sequence generation
- **Sequence Scanning**: Find motif matches above configurable thresholds
- **CLI Interface**: Easy-to-use command-line tool
- **Simulation**: Planted-motif generator for testing and validation

## Installation

```bash
pip install -e ".[dev]"
```

## Quick Start

```bash
# Find motifs in FASTA sequences
motif-finder sequences.fasta --width 8

# With specific algorithm
motif-finder sequences.fasta --width 10 --algorithm gibbs

# Run simulation tests
python -m bio_motif_finder.simulate
```

## Algorithms

### Greedy Median-String
- Brute-force approach for small motif widths (≤8)
- Finds the median string minimizing total Hamming distance
- Guaranteed optimal for small widths

### Gibbs Sampling
- Stochastic algorithm for larger motifs
- Iteratively samples motif occurrences
- Good for motifs with variable spacing

### MEME-lite (EM-style)
- Expectation-Maximization approach
- Builds Position Weight Matrix iteratively
- Handles motifs with position-specific information content

## PWM Scoring

The toolkit uses information content scoring:
- Log-odds scores against background model
- Relative entropy for motif significance
- Configurable thresholds for match detection

## Testing

```bash
# Run all tests
pytest

# Run with coverage
pytest --cov=bio_motif_finder

# Run specific test
pytest tests/test_pwm.py -v
```

## Project Structure

```
bio-motif-finder-py/
├── src/
│   └── bio_motif_finder/
│       ├── __init__.py
│       ├── pwm.py          # Position Weight Matrix
│       ├── score.py        # Scoring functions
│       ├── greedy.py       # Greedy algorithm
│       ├── gibbs.py        # Gibbs sampling
│       ├── meme.py         # EM-style algorithm
│       ├── simulate.py     # Test data generation
│       └── cli.py          # Command-line interface
├── tests/
│   ├── test_pwm.py
│   ├── test_greedy.py
│   ├── test_gibbs.py
│   ├── test_meme.py
│   ├── test_simulate.py
│   └── test_cli.py
├── pyproject.toml
└── README.md
```

## License

MIT License
