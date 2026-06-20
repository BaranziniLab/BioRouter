# bio-blast-lite-rs

A BLAST-like local sequence similarity search tool written in Rust.

## Overview

`blast-lite` implements the classic seed-and-extend paradigm for local sequence alignment:

1. **Index** a FASTA database with a k-mer/word index (HashMap from k-mer → list of (sequence, position) hits).
2. **Seed** — extract all query k-mers and look them up in the index to find exact word matches.
3. **Cluster** seeds along diagonals to group redundant hits from the same alignment region.
4. **Extend** each seed cluster with ungapped extension (X-drop) followed by banded Smith-Waterman for gapped alignment.
5. **Score** each alignment: compute raw score, percent identity, bit score, and E-value (Karlin–Altschul statistics).
6. **Rank** hits by score, using independent seed support as a tie-breaker, and report the top results.

## Modules

| Module | Purpose |
|--------|---------|
| `fasta` | FASTA parsing (multi-record, file/string/reader), writing, roundtrip |
| `index` | K-mer inverted index (HashMap<Vec\<u8\>, Vec\<KmerHit\>>) with ambiguity support |
| `seed` | Seed extraction from query, diagonal clustering |
| `extend` | Ungapped extension (X-drop) + banded Smith-Waterman (gapped) |
| `score` | Nucleotide match/mismatch scoring, BLOSUM62 substitution matrix |
| `stats` | Alignment statistics: percent identity, bit score, E-value |
| `search` | Pipeline orchestrator: seed → cluster → extend → score → merge → rank |
| `cli` | CLI with `index` and `search` subcommands (clap) |

## Algorithm Notes

### Seed Finding
Every overlapping k-mer window of the query is looked up in the k-mer index. Each database occurrence becomes a `SeedHit` with (db_seq_idx, db_pos, query_pos). Seeds are then clustered by database sequence and diagonal proximity (within `band_width` diagonals) to group hits from the same alignment.

### Ungapped Extension
From each seed cluster representative, extend left and right along the diagonal scoring matches (+2) and mismatches (-3). Stop when the running score drops more than `x_drop` below the best score seen so far. This is the standard BLAST X-drop heuristic.

### Gapped Extension (Banded Smith-Waterman)
Around the ungapped region center, perform dynamic programming within a diagonal band of half-width `band_width`. This constrains the O(n²) Smith-Waterman to O(n × band_width). Uses affine gap penalties (gap_open + gap_extend per gap). The DP stores traceback pointers for alignment reconstruction.

### E-value Calculation
Uses approximate Karlin–Altschul parameters (λ ≈ 1.28, K ≈ 0.46 for nucleotides):
- **Bit score**: S' = (λ·S − ln K) / ln 2
- **E-value**: E = K · m · n · e^(−λ·S), where m = query length, n = total database size

### Hit Merging and Ranking
Hits from the same database sequence that overlap in query coordinates are merged, keeping the best-scoring alignment and accumulating `seed_support` (count of independent seed clusters). Hits are sorted by score descending, then by seed_support descending as a tie-breaker.

## CLI Usage

```bash
# Build
cargo build --release

# Index a database
cargo run -- index -d database.fasta -k 11

# Search a query against a database
cargo run -- search -q query.fasta -d database.fasta -k 11 --format both

# Custom parameters
cargo run -- search -q query.fasta -d database.fasta \
    -k 4 --x-drop 15 --band-width 32 --e-value 0.001 -f tabular
```

### CLI Options

| Flag | Default | Description |
|------|---------|-------------|
| `-k, --word-size` | 11 | k-mer size for seeding |
| `--x-drop` | 10 | X-drop threshold for ungapped extension |
| `--band-width` | 16 | Half-width of the diagonal band for gapped SW |
| `--flank` | 50 | Flank size around ungapped region for gapped SW |
| `--e-value` | 10.0 | Maximum E-value threshold |
| `-n, --max-hits` | 500 | Maximum hits to report |
| `--match-score` | 2 | Nucleotide match score |
| `--mismatch-score` | -3 | Nucleotide mismatch penalty |
| `--gap-open` | 5 | Gap opening penalty |
| `--gap-extend` | 2 | Gap extension penalty |
| `-f, --format` | both | Output format: `tabular`, `alignments`, or `both` |

## Tests

```bash
cargo test
```

60 tests covering:
- FASTA parsing (single/multi-record, whitespace, roundtrip, ambiguity codes, proteins)
- K-mer index (build, lookup, ambiguity, stats)
- Seed finding (exact match, no match, partial, clustering)
- Extension (ungapped X-drop, banded SW exact match, with gaps, no match)
- Scoring (nucleotide, BLOSUM62, custom)
- Statistics (percent identity, E-value, gap handling)
- Search pipeline (exact match, no match, partial, multi-DB, hit sorting, tabular output)
- Integration (exact match, no match, known alignment, seed-extension, multi-hit ranking, FASTA I/O, large database, configurable parameters, E-value filtering)
