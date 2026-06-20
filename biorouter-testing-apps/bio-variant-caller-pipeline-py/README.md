# bio-variant-caller-pipeline-py

A pure-Python variant-calling pipeline with no external bioinformatics dependencies.

## Architecture

```
src/bio_variant_caller/
├── __init__.py      # Package init
├── models.py        # Data models (AlignedRead, PileupPosition, Variant)
├── phred.py         # Phred-quality arithmetic
├── pileup.py        # Reference-aware pileup engine
├── caller.py        # Bayesian genotype caller (AA/AB/BB)
├── vcf.py           # VCF 4.2 output writer
├── annotate.py      # ts/tv, allele balance, strand balance
├── simulate.py      # Read simulator with ground-truth injection
└── cli.py           # Command-line interface
```

## Pipeline

```
Reference + Reads → Pileup Engine → Variant Caller → Annotator → VCF
```

1. **Pileup Engine** (`pileup.py`): Walks CIGAR strings to align reads to the reference, building per-position base counts with strand and quality information.
2. **Variant Caller** (`caller.py`): Evaluates diploid genotypes (AA/AB/BB) using a Bayesian likelihood model with Phred-scaled base qualities. Configurable thresholds for depth, allele frequency, base quality, and genotype quality.
3. **Annotator** (`annotate.py`): Adds transition/transversion classification, allele balance, strand balance.
4. **VCF Writer** (`vcf.py`): Outputs standard VCF 4.2 format with INFO and FORMAT fields.

## Usage

```bash
# Install in development mode
pip install -e ".[dev]"

# Simulate reads with known variants
biovariantcall simulate \
  -r reference.fa \
  -o reads.tsv \
  -t truth.tsv \
  -c 30 \
  --variants 10:A:G 30:C:T

# Run the pipeline
biovariantcall run \
  -r reference.fa \
  -R reads.tsv \
  -o output.vcf \
  --stats stats.json

# Evaluate against truth
biovariantcall eval \
  -v output.vcf \
  -t truth.tsv
```

## Running Tests

```bash
pip install -e ".[dev]"
pytest -v
```

## Features

- **Pileup engine**: Full CIGAR support (M/I/D/S/H/N/P), quality-weighted counts, strand tracking
- **Bayesian caller**: Diploid genotype model, Phred-scaled quality scores, configurable filters
- **VCF output**: Standard 4.2 format with DP, AF, TSTV, AB, SB in INFO; GT:GQ:DP:AD in samples
- **Annotation**: ts/tv classification, allele balance, strand balance
- **Simulator**: Configurable coverage, error rates, read lengths, random seed reproducibility
- **CLI**: simulate → run → eval workflow with stats output
- **Tests**: Sensitivity/precision evaluation, edge cases (low depth, strand bias, homopolymers)
