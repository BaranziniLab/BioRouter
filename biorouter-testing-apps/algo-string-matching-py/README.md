# strmatch — String-Matching & Text-Indexing Library

A pure-Python library implementing classical string-matching algorithms with a
CLI for searching text files and benchmarking algorithms.

## Features

### Exact Single-Pattern Matching

| Algorithm              | Preprocessing      | Search             | Notes                            |
|------------------------|--------------------|--------------------|----------------------------------|
| Naive                  | O(1)               | O(n·m)             | Brute-force baseline             |
| Knuth-Morris-Pratt     | O(m)               | O(n + m)           | Failure-function automaton       |
| Boyer-Moore            | O(m + σ)           | O(n·m) worst, ~O(n/m) avg | Bad-character + good-suffix |
| Rabin-Karp             | O(m)               | O(n + m) expected   | Rolling hash, Monte Carlo        |
| Finite Automaton       | O(m·σ)             | O(n)               | δ-table precomputed              |

### Multi-Pattern Matching

| Algorithm              | Preprocessing      | Search             | Notes                            |
|------------------------|--------------------|--------------------|----------------------------------|
| Aho-Corasick           | O(Σ|pᵢ|)          | O(n + z)           | Trie + failure + output links    |

### Indexing

| Data Structure / Algo  | Construction       | Query              | Notes                            |
|------------------------|--------------------|--------------------|----------------------------------|
| Suffix Array + LCP     | O(n log n)         | O(m log n)         | Binary search on suffixes        |
| Z-Algorithm            | O(n)               | —                  | Computes Z-array for pattern joining |
| Longest Common Substr. | O(n log n)         | —                  | Via suffix array + LCP           |
| Longest Repeated Substr| O(n log n)         | —                  | Via suffix array + LCP           |

### Approximate Matching

| Algorithm              | Time               | Space              | Notes                            |
|------------------------|--------------------|--------------------|----------------------------------|
| Edit Distance (Lev.)   | O(n·m)             | O(min(n,m))        | Wagner-Fischer, full matrix      |
| k-Mismatch Search      | O(n·m)             | O(n)               | Bounded Hamming distance         |

*n = text length, m = pattern length, σ = alphabet size, z = number of matches*

## Quickstart

```bash
git clone <repo-url> && cd algo-string-matching-py
python -m venv .venv && source .venv/bin/activate
pip install -e ".[dev]"     # installs strmatch + pytest

# Use the library
python -c "from strmatch.exact.kmp import kmp_search; print(kmp_search('ABABABAB', 'ABAB'))"

# Run the CLI
strmatch search "pattern" textfile.txt --algo kmp

# Run tests (works from a clean checkout — no install required thanks to pyproject.toml pythonpath)
pytest -v
```

## CLI Usage

### Search mode
```bash
# Search a pattern in a file using a specific algorithm
strmatch search "pattern" textfile.txt --algo kmp

# Search patterns from a file
strmatch search --patterns patterns.txt textfile.txt --algo aho-corasick

# Show timing information
strmatch search "ATCG" genome.txt --algo boyer-moore --time
```

### Compare mode
```bash
# Benchmark all algorithms on the same input
strmatch compare "pattern" textfile.txt

# Compare with specific algorithms
strmatch compare "pattern" textfile.txt --algos naive,kmp,boyer-moore
```

## Running Tests

`pytest` works out of the box from a clean clone — the `[tool.pytest.ini_options]`
section in `pyproject.toml` sets `pythonpath = ["src"]` so no `pip install` is
required.

```bash
pytest -v
```

## Algorithm Notes

### Knuth-Morris-Pratt (KMP)
Builds a failure function (partial match table) that tells us how much of the
current match can be reused when a mismatch occurs. Guaranteed O(n+m) time.

### Boyer-Moore
Scans the pattern from right to left. Two heuristics:
- **Bad-character rule**: skip alignments based on mismatched text character.
- **Good-suffix rule**: skip alignments based on matched suffix structure.
In practice, sublinear for large alphabets.

### Rabin-Karp
Computes a rolling hash over the pattern and each m-length window of the text.
When hashes match, verifies character-by-character (Las Vegas variant).

### Aho-Corasick
Builds a trie of all patterns, then adds failure links (BFS) and output links
to create a finite-state machine that matches all patterns simultaneously.

### Suffix Array
Sorted array of all suffixes. Combined with the LCP array (longest common
prefix between adjacent suffixes), supports efficient substring queries and
derives longest common/repeated substrings.

## License

MIT
