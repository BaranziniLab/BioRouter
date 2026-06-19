# bio-kmer-counter-cpp

A k-mer counting and de Bruijn graph toolkit in modern C++17.

## Features

- **K-mer counting**: Hash-map based counter with 2-bit encoding of nucleotides (A=00, C=01, G=10, T=11)
- **Canonical k-mers**: Strand-independent representation (lexicographically smaller of k-mer and its reverse complement)
- **De Bruijn graph**: Node/edge structures with unitig traversal for contig assembly
- **Sequence utilities**: GC content, sequence complexity, FASTA/FASTQ parsing
- **CLI interface**: Count k-mers, assemble contigs, show sequence info

## Build

```bash
# Create build directory
mkdir build && cd build

# Configure
cmake ..

# Build
cmake --build .

# Run tests
ctest --output-on-failure

# Run benchmark
./bkc_benchmark
```

## Usage

### Count k-mers
```bash
# Count k-mers with k=21 (default) from a FASTA file
./bio-kmer-counter count input.fa

# Count with k=31 and minimum coverage filter
./bio-kmer-counter count -k 31 -c 2 input.fa

# Suppress histogram
./bio-kmer-counter count --no-spectrum input.fq
```

### Assemble contigs
```bash
# Assemble contigs from k-mers
./bio-kmer-counter assemble input.fa

# Assemble with k=31 and minimum coverage 3
./bio-kmer-counter assemble -k 31 -c 3 input.fa

# Limit to top 10 contigs
./bio-kmer-counter assemble -n 10 input.fa
```

### Sequence info
```bash
# Show GC content and complexity
./bio-kmer-counter info input.fa
```

## Input Formats

### FASTA
```
>sequence_id [optional description]
ACGTACGTACGT...
TGCAACGTACGT...
```

### FASTQ
```
@read_id [optional description]
ACGTACGT
+
IIIIIIII
```

- Multi-line sequences are supported
- Both `.fa`, `.fasta`, `.fna` (FASTA) and `.fq`, `.fastq` (FASTQ) extensions are recognized
- Format is auto-detected from extension or file content

## Architecture

### Modules

| Module | Description |
|--------|-------------|
| `kmer.hpp/.cpp` | 2-bit nucleotide encoding, canonical k-mers, GC/complexity |
| `counter.hpp/.cpp` | Hash-map based k-mer counting with spectrum generation |
| `dbg.hpp/.cpp` | De Bruijn graph construction and unitig traversal assembly |
| `io.hpp/.cpp` | FASTA/FASTQ parser with streaming support |
| `cli.hpp/.cpp` | Command-line interface |

### Data Structures

- **k-mer encoding**: Each nucleotide is encoded in 2 bits, packed into a `uint64_t` (supports k ≤ 32)
- **Canonical k-mers**: The lexicographically smaller of a k-mer and its reverse complement
- **De Bruijn graph nodes**: `(k-1)`-mers with in/out degree tracking
- **De Bruijn graph edges**: k-mers connecting prefix/suffix `(k-1)`-mers

### Assembly Algorithm

1. Count canonical k-mers from input sequences
2. Build de Bruijn graph from k-mers with count ≥ minimum coverage
3. Traverse unitigs (maximal non-branching paths)
4. Reconstruct contig sequences from unitig paths

## Testing

The test suite covers:
- 2-bit encoding round-trip correctness
- Canonical k-mer computation
- Reverse complement correctness
- K-mer counting with known sequences
- FASTA/FASTQ parsing
- De Bruijn graph construction
- Contig assembly reconstruction

Run tests with:
```bash
cd build
ctest --output-on-failure
```

Or run the test binary directly:
```bash
./bkc_tests
```

## Performance

The benchmark (`bkc_benchmark`) measures:
- Encode/decode throughput (10M operations)
- Canonical k-mer computation (10M operations)
- K-mer counting on sequences up to 1M bp
- De Bruijn graph build and assembly time

## License

This is an open-source software project developed as part of the BioRouter ecosystem.
