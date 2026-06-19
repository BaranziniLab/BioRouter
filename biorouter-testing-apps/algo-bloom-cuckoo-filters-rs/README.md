# algo-bloom-cuckoo-filters-rs

A comprehensive probabilistic data structures library in Rust, implementing Bloom filters, Counting Bloom filters, Cuckoo filters, and Scalable Bloom filters with empirical analysis utilities and benchmarking.

## Features

- **Bloom Filter** — Classic probabilistic set with configurable bits/hashes and optimal parameter sizing from expected element count and target false-positive rate.
- **Counting Bloom Filter** — Extends Bloom filter with 4-bit counters enabling element removal.
- **Cuckoo Filter** — Space-efficient filter with fingerprints, two candidate buckets, and kick-out relocation on collision. Supports deletion.
- **Scalable Bloom Filter** — Automatically grows by adding successive Bloom filter layers with progressively tighter FPR budgets, maintaining the overall target FPR.
- **Pluggable Hashing** — Generic over hashable items with a pluggable multi-hash trait (`BuildMultiHasher`).
- **Empirical Analysis** — Measure actual FPR vs theoretical, compare all structures side-by-side.
- **Benchmarking** — Insert and query throughput measurement across all filter types.
- **Serialization** — All filters support JSON serialization/deserialization via serde.
- **CLI Demo** — Interactive demonstration of all features.

## Project Structure

```
src/
├── lib.rs          — Library root, re-exports
├── hashing.rs      — Pluggable hasher trait + DoubleHasher (Kirsch-Mitzenmacher)
├── bloom.rs        — Classic Bloom filter (optimal sizing, insert, contains)
├── counting.rs     — Counting Bloom filter (4-bit counters, removal)
├── cuckoo.rs       — Cuckoo filter (fingerprints, buckets, relocation)
├── scalable.rs     — Scalable Bloom filter (auto-growing layers)
├── analysis.rs     — FPR analysis utilities + benchmark runner
└── bin/
    └── demo.rs     — CLI demonstration
tests/
└── integration_tests.rs — Comprehensive test suite (property + integration)
```

## Quick Start

```rust
use algo_bloom_cuckoo_filters_rs::bloom::BloomFilter;

fn main() {
    // Create a Bloom filter optimized for 10,000 items at 1% FPR
    let mut bf = BloomFilter::optimal(10_000, 0.01);

    // Insert items
    for i in 0..10_000 {
        bf.insert(&i);
    }

    // Query — no false negatives guaranteed
    assert!(bf.contains(&42));

    // Check theoretical FPR
    println!("Theoretical FPR: {:.6}", bf.theoretical_fpr());
}
```

### Cuckoo Filter with Deletion

```rust
use algo_bloom_cuckoo_filters_rs::cuckoo::CuckooFilter;

let mut cf = CuckooFilter::new(10_000);
cf.insert(&"hello");
cf.insert(&"world");

assert!(cf.contains(&"hello"));
cf.delete(&"hello");
assert!(!cf.contains(&"hello"));
```

### Scalable Bloom Filter

```rust
use algo_bloom_cuckoo_filters_rs::scalable::ScalableBloomFilter;

let mut sbf = ScalableBloomFilter::new(100, 0.01);
for i in 0..10_000 {
    sbf.insert(&i);
}
println!("Layers: {}, Total bits: {}", sbf.num_layers(), sbf.total_bits());
```

## Running

```bash
# Build
cargo build --release

# Run the demo
cargo run --bin demo

# Run all tests
cargo test

# Run with output
cargo test -- --nocapture
```

## Tests

The test suite includes:

- **No false negatives** — Every inserted item is always found
- **FPR within tolerance** — Measured FPR stays within bounds of theoretical target
- **Cuckoo eviction correctness** — Items survive relocation under pressure
- **Serialization round-trip** — All filters survive JSON encode/decode
- **Property tests** — Randomized inputs across types (strings, integers, floats, byte slices)
- **Edge cases** — Single-item filters, insert/remove cycles, high load factors

## Theory

### Bloom Filter
- **Bits**: m = -(n · ln(p)) / (ln 2)²
- **Hashes**: k = (m/n) · ln 2
- **FPR**: (1 - e^(-kn/m))^k

### Counting Bloom Filter
Same as Bloom but with 4-bit counters instead of bits. Removal decrements counters.

### Cuckoo Filter
- Fingerprint: 16-bit hash
- Two candidate buckets per item: i1 = hash(item), i2 = i1 ⊕ hash(fingerprint)
- Relocation: up to 500 kick-outs before failure
- FPR ≈ 2·b / 2^f where b = bucket size, f = fingerprint bits

### Scalable Bloom Filter
Sequential layers with tightening FPR:
- Layer i FPR budget: p · r^i (where r = 0.5)
- Layer i capacity: n · 2^i
- Overall FPR maintained within target

## License

MIT
