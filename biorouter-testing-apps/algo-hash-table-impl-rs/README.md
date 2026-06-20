# algo-hash-table-impl-rs

Hash table library in Rust implementing multiple collision-resolution strategies,
with benchmarks, property-style tests, and a CLI demo.

## Collision Strategies

| Strategy | Module | Probe Style | Deletion Handling |
|----------|--------|-------------|-------------------|
| Separate Chaining | `chaining` | Bucket-level linked list/vector | Direct removal |
| Linear Probing | `linear` | Linear scan from hash slot | Tombstone markers |
| Robin Hood Hashing | `robinhood` | Linear with displacement tracking | Backward-shift deletion |

All three are generic over `<K, V, S>` (key, value, hasher) and expose a unified
`HashMap` trait with `insert`, `get`, `get_mut`, `remove`, `len`, `is_empty`,
`capacity`, `load_factor`, `iter`, `keys`, `values`, and `clear`.

## Modules

```
src/
├── lib.rs                  # Re-exports all modules
├── common.rs               # HashMap trait, default hasher, config
├── chaining/mod.rs         # Separate-chaining HashMap
├── linear/mod.rs           # Open-addressing with linear probing
├── linear/tests.rs         # Unit + invariant tests for linear probing
├── robinhood/mod.rs        # Robin Hood hashing HashMap
├── robinhood/tests.rs      # Unit + invariant tests for Robin Hood
├── cli/main.rs             # CLI demo binary
├── cluster_analysis.rs     # Collision cluster analysis utilities
├── tests/chaining.rs       # Property-style tests for chaining
├── tests/linear.rs         # Property-style tests for linear probing
├── tests/robinhood.rs      # Property-style tests for Robin Hood
├── tests/common.rs         # Shared test helpers
└── tests/integration.rs    # Cross-implementation invariant tests
benches/
└── hash_table_bench.rs     # Criterion benchmarks
```

## Quick Start

```bash
# Build the library and demo binary
cargo build --release

# Run the full test suite
cargo test

# Run benchmarks
cargo bench

# CLI demo (inserts 10k entries into each implementation, shows stats)
cargo run --bin hashtbl-demo
```

## Benchmark Workloads

The benchmark suite (`benches/hash_table_bench.rs`) compares all three
implementations against `std::collections::HashMap` across:

- **Sequential insertion** (1k, 10k entries)
- **Random insertion** (1k, 10k entries)
- **Lookup hit** (pre-populated table, random lookups)
- **Lookup miss** (keys not in table)
- **Mixed workload** (50% insert / 50% lookup)
- **Deletion** (remove all entries from a populated table)
- **Iteration** (iterate over all entries)

## Load-Factor Tuning

All maps default to a max load factor of 0.75. Configure via
`with_capacity_and_load_factor(capacity, max_load)`.

## False-Positive / Cluster Analysis

`cluster_analysis::analyze()` runs each strategy against a collision-heavy
hasher and reports:
- Cluster count (contiguous occupied runs)
- Max cluster length
- Average probe length for successful/unsuccessful lookups
- Tombstone ratio (open-addressing strategies)

## License

MIT
