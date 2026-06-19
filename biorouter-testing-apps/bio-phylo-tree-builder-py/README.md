# bio-phylo

A molecular phylogenetics toolkit in Python for distance-based and parsimony tree construction.

## Features

### Tree Construction Methods
- **UPGMA** — Unweighted Pair Group Method with Arithmetic Mean (ultrametric trees, constant molecular clock)
- **Neighbor-Joining (NJ)** — Saitou & Nei algorithm (additive trees, no clock assumption)
- **Maximum Parsimony** — Fitch algorithm with greedy stepwise addition heuristic

### Distance Models
- **p-distance** — Proportion of differing sites
- **Jukes-Cantor (JC69)** — Single-parameter model correcting for multiple hits
- **Kimura 2-parameter (K2P)** — Two-parameter model distinguishing transitions and transversions

### Tree Operations
- Newick parsing and serialization
- Multiple traversals (preorder, postorder, level-order)
- Tree rooting and rerooting
- Clade queries, MRCA finding, topology analysis
- Bootstrap support estimation
- ASCII tree rendering

## Installation

```bash
# Clone the repository
git clone <repo-url>
cd bio-phylo-tree-builder-py

# Create virtual environment
python3 -m venv .venv
source .venv/bin/activate

# Install in development mode
pip install -e ".[dev]"
```

## Usage

### Build a tree from FASTA alignment

```bash
# Neighbor-Joining with p-distance
bio-phylo build --input alignment.fasta --method nj

# UPGMA with Kimura 2-parameter model
bio-phylo build --input alignment.fasta --method upgma --model kimura-2param

# Maximum Parsimony
bio-phylo build --input alignment.fasta --method parsimony

# With bootstrap support (100 replicates)
bio-phylo build --input alignment.fasta --method nj --bootstrap 100

# Save Newick to file
bio-phylo build --input alignment.fasta --method nj --output tree.nwk
```

### Build from distance matrix

```bash
bio-phylo build --matrix distances.txt --method upgma
```

### Compute pairwise distances

```bash
bio-phylo distance --input alignment.fasta --model kimura-2param
```

### Analyze a Newick tree

```bash
bio-phylo info "((A:0.1,B:0.2):0.3,C:0.4);"
```

### Python API

```python
from bio_phylo.distance import compute_distance_matrix, parse_fasta
from bio_phylo.nj import neighbor_joining
from bio_phylo.upgma import upgma
from bio_phylo.parsimony import parsimony_greedy, fitch_score
from bio_phylo.bootstrap import bootstrap_support, annotate_tree_with_support
from bio_phylo.ascii_tree import render_tree_compact
from bio_phylo.tree import from_newick

# Read alignment
alignment = parse_fasta(open("alignment.fasta").read())

# Build tree
dm = compute_distance_matrix(alignment, model="k2p")
tree = neighbor_joining(dm)

# Or with UPGMA
tree = upgma(dm)

# Or parsimony
tree = parsimony_greedy(alignment)

# Compute bootstrap support
support = bootstrap_support(
    alignment,
    tree_builder=lambda aln: neighbor_joining(
        compute_distance_matrix(aln, model="k2p")
    ),
    n_replicates=100,
)
tree = annotate_tree_with_support(tree, support, 100)

# Output
print(tree.to_newick())
print(render_tree_compact(tree))
```

## Running Tests

```bash
# Run all tests
python -m pytest tests/ -v

# Run with coverage
python -m pytest tests/ --cov=bio_phylo --cov-report=term-missing
```

## Project Structure

```
bio-phylo-tree-builder-py/
├── pyproject.toml              # Package configuration
├── README.md                   # This file
├── src/
│   └── bio_phylo/
│       ├── __init__.py         # Package metadata
│       ├── tree.py             # Tree data structure, Newick parser
│       ├── distance.py         # Distance matrix, substitution models
│       ├── upgma.py            # UPGMA algorithm
│       ├── nj.py               # Neighbor-Joining algorithm
│       ├── parsimony.py        # Fitch parsimony
│       ├── bootstrap.py        # Bootstrap support
│       ├── ascii_tree.py       # ASCII tree rendering
│       ├── cli.py              # Command-line interface
│       └── utils.py            # FASTA I/O, validation
└── tests/
    ├── test_tree.py            # Tree operations, Newick round-trip
    ├── test_distance.py        # Distance models, matrix operations
    ├── test_upgma.py           # UPGMA correctness
    ├── test_nj.py              # Neighbor-Joining correctness
    ├── test_parsimony.py       # Fitch scoring, greedy heuristic
    ├── test_bootstrap.py       # Bootstrap support
    ├── test_ascii_tree.py      # ASCII rendering
    ├── test_cli.py             # CLI integration
    └── test_utils.py           # I/O and validation
```

## License

MIT
