//! Criterion benchmark suite for all hash table implementations.
//!
//! Compares ChainingHashMap, LinearProbingHashMap, RobinHoodHashMap, and
//! std::collections::HashMap across several workloads and load factors.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::prelude::*;
use rand::rngs::StdRng;

use algo_hash_table_impl_rs::chaining::ChainingHashMap;
use algo_hash_table_impl_rs::common::HashMap as HashMapTrait;
use algo_hash_table_impl_rs::linear::LinearProbingHashMap;
use algo_hash_table_impl_rs::robinhood::RobinHoodHashMap;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn random_keys(n: usize, seed: u64) -> Vec<u64> {
    let mut rng = StdRng::seed_from_u64(seed);
    (0..n).map(|_| rng.gen_range(0..u64::MAX)).collect()
}

// ---------------------------------------------------------------------------
// Benchmark: sequential insert
// ---------------------------------------------------------------------------

fn bench_sequential_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequential_insert");

    for size in [1_000, 10_000] {
        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(BenchmarkId::new("chaining", size), &size, |b, &s| {
            b.iter(|| {
                let mut m = ChainingHashMap::<u64, u64>::with_capacity(s);
                for i in 0..s as u64 {
                    m.insert(i, i);
                }
                black_box(&m);
            })
        });

        group.bench_with_input(BenchmarkId::new("linear", size), &size, |b, &s| {
            b.iter(|| {
                let mut m = LinearProbingHashMap::<u64, u64>::with_capacity(s);
                for i in 0..s as u64 {
                    m.insert(i, i);
                }
                black_box(&m);
            })
        });

        group.bench_with_input(BenchmarkId::new("robinhood", size), &size, |b, &s| {
            b.iter(|| {
                let mut m = RobinHoodHashMap::<u64, u64>::with_capacity(s);
                for i in 0..s as u64 {
                    m.insert(i, i);
                }
                black_box(&m);
            })
        });

        group.bench_with_input(BenchmarkId::new("std_hashmap", size), &size, |b, &s| {
            b.iter(|| {
                let mut m = std::collections::HashMap::with_capacity(s);
                for i in 0..s as u64 {
                    m.insert(i, i);
                }
                black_box(&m);
            })
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark: random insert
// ---------------------------------------------------------------------------

fn bench_random_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("random_insert");

    for size in [1_000, 10_000] {
        let keys = random_keys(size, 99);
        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(BenchmarkId::new("chaining", size), &keys, |b, keys| {
            b.iter(|| {
                let mut m = ChainingHashMap::<u64, u64>::with_capacity(keys.len());
                for &k in keys {
                    m.insert(k, k);
                }
                black_box(&m);
            })
        });

        group.bench_with_input(BenchmarkId::new("linear", size), &keys, |b, keys| {
            b.iter(|| {
                let mut m = LinearProbingHashMap::<u64, u64>::with_capacity(keys.len());
                for &k in keys {
                    m.insert(k, k);
                }
                black_box(&m);
            })
        });

        group.bench_with_input(BenchmarkId::new("robinhood", size), &keys, |b, keys| {
            b.iter(|| {
                let mut m = RobinHoodHashMap::<u64, u64>::with_capacity(keys.len());
                for &k in keys {
                    m.insert(k, k);
                }
                black_box(&m);
            })
        });

        group.bench_with_input(BenchmarkId::new("std_hashmap", size), &keys, |b, keys| {
            b.iter(|| {
                let mut m = std::collections::HashMap::with_capacity(keys.len());
                for &k in keys {
                    m.insert(k, k);
                }
                black_box(&m);
            })
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark: lookup (hit)
// ---------------------------------------------------------------------------

fn bench_lookup_hit(c: &mut Criterion) {
    let mut group = c.benchmark_group("lookup_hit");

    for size in [1_000, 10_000] {
        let keys: Vec<u64> = (0..size as u64).collect();
        let query_keys = random_keys(1000, 42);

        // Pre-populate.
        let mut cm = ChainingHashMap::<u64, u64>::with_capacity(size);
        let mut lm = LinearProbingHashMap::<u64, u64>::with_capacity(size);
        let mut rm = RobinHoodHashMap::<u64, u64>::with_capacity(size);
        let mut sm = std::collections::HashMap::with_capacity(size);
        for &k in &keys {
            cm.insert(k, k);
            lm.insert(k, k);
            rm.insert(k, k);
            sm.insert(k, k);
        }

        group.throughput(Throughput::Elements(1000));

        group.bench_with_input(BenchmarkId::new("chaining", size), &query_keys, |b, qk| {
            b.iter(|| {
                for &k in qk {
                    black_box(cm.get(&k));
                }
            })
        });

        group.bench_with_input(BenchmarkId::new("linear", size), &query_keys, |b, qk| {
            b.iter(|| {
                for &k in qk {
                    black_box(lm.get(&k));
                }
            })
        });

        group.bench_with_input(BenchmarkId::new("robinhood", size), &query_keys, |b, qk| {
            b.iter(|| {
                for &k in qk {
                    black_box(rm.get(&k));
                }
            })
        });

        group.bench_with_input(BenchmarkId::new("std_hashmap", size), &query_keys, |b, qk| {
            b.iter(|| {
                for &k in qk {
                    black_box(sm.get(&k));
                }
            })
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark: lookup (miss)
// ---------------------------------------------------------------------------

fn bench_lookup_miss(c: &mut Criterion) {
    let mut group = c.benchmark_group("lookup_miss");

    for size in [1_000, 10_000] {
        let keys: Vec<u64> = (0..size as u64).collect();
        // Keys that are NOT in the map.
        let miss_keys: Vec<u64> = (size as u64..size as u64 + 1000).collect();

        let mut cm = ChainingHashMap::<u64, u64>::with_capacity(size);
        let mut lm = LinearProbingHashMap::<u64, u64>::with_capacity(size);
        let mut rm = RobinHoodHashMap::<u64, u64>::with_capacity(size);
        let mut sm = std::collections::HashMap::with_capacity(size);
        for &k in &keys {
            cm.insert(k, k);
            lm.insert(k, k);
            rm.insert(k, k);
            sm.insert(k, k);
        }

        group.throughput(Throughput::Elements(1000));

        group.bench_with_input(BenchmarkId::new("chaining", size), &miss_keys, |b, mk| {
            b.iter(|| {
                for &k in mk {
                    black_box(cm.get(&k));
                }
            })
        });

        group.bench_with_input(BenchmarkId::new("linear", size), &miss_keys, |b, mk| {
            b.iter(|| {
                for &k in mk {
                    black_box(lm.get(&k));
                }
            })
        });

        group.bench_with_input(BenchmarkId::new("robinhood", size), &miss_keys, |b, mk| {
            b.iter(|| {
                for &k in mk {
                    black_box(rm.get(&k));
                }
            })
        });

        group.bench_with_input(BenchmarkId::new("std_hashmap", size), &miss_keys, |b, mk| {
            b.iter(|| {
                for &k in mk {
                    black_box(sm.get(&k));
                }
            })
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark: mixed workload (50% insert, 50% lookup)
// ---------------------------------------------------------------------------

fn bench_mixed_workload(c: &mut Criterion) {
    let mut group = c.benchmark_group("mixed_workload");

    let size = 5_000;
    let keys = random_keys(size * 2, 77);

    group.throughput(Throughput::Elements(size as u64));

    group.bench_function("chaining", |b| {
        b.iter(|| {
            let mut m = ChainingHashMap::<u64, u64>::with_capacity(size);
            for i in 0..size {
                if i % 2 == 0 {
                    m.insert(keys[i], keys[i]);
                } else {
                    black_box(m.get(&keys[i / 2]));
                }
            }
        })
    });

    group.bench_function("linear", |b| {
        b.iter(|| {
            let mut m = LinearProbingHashMap::<u64, u64>::with_capacity(size);
            for i in 0..size {
                if i % 2 == 0 {
                    m.insert(keys[i], keys[i]);
                } else {
                    black_box(m.get(&keys[i / 2]));
                }
            }
        })
    });

    group.bench_function("robinhood", |b| {
        b.iter(|| {
            let mut m = RobinHoodHashMap::<u64, u64>::with_capacity(size);
            for i in 0..size {
                if i % 2 == 0 {
                    m.insert(keys[i], keys[i]);
                } else {
                    black_box(m.get(&keys[i / 2]));
                }
            }
        })
    });

    group.bench_function("std_hashmap", |b| {
        b.iter(|| {
            let mut m = std::collections::HashMap::with_capacity(size);
            for i in 0..size {
                if i % 2 == 0 {
                    m.insert(keys[i], keys[i]);
                } else {
                    black_box(m.get(&keys[i / 2]));
                }
            }
        })
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark: deletion
// ---------------------------------------------------------------------------

fn bench_deletion(c: &mut Criterion) {
    let mut group = c.benchmark_group("deletion");

    for size in [1_000, 10_000] {
        let keys: Vec<u64> = (0..size as u64).collect();
        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(BenchmarkId::new("chaining", size), &keys, |b, keys| {
            b.iter(|| {
                let mut m = ChainingHashMap::<u64, u64>::with_capacity(keys.len());
                for &k in keys {
                    m.insert(k, k);
                }
                for &k in keys {
                    m.remove(&k);
                }
                black_box(&m);
            })
        });

        group.bench_with_input(BenchmarkId::new("linear", size), &keys, |b, keys| {
            b.iter(|| {
                let mut m = LinearProbingHashMap::<u64, u64>::with_capacity(keys.len());
                for &k in keys {
                    m.insert(k, k);
                }
                for &k in keys {
                    m.remove(&k);
                }
                black_box(&m);
            })
        });

        group.bench_with_input(BenchmarkId::new("robinhood", size), &keys, |b, keys| {
            b.iter(|| {
                let mut m = RobinHoodHashMap::<u64, u64>::with_capacity(keys.len());
                for &k in keys {
                    m.insert(k, k);
                }
                for &k in keys {
                    m.remove(&k);
                }
                black_box(&m);
            })
        });

        group.bench_with_input(BenchmarkId::new("std_hashmap", size), &keys, |b, keys| {
            b.iter(|| {
                let mut m = std::collections::HashMap::with_capacity(keys.len());
                for &k in keys {
                    m.insert(k, k);
                }
                for &k in keys {
                    m.remove(&k);
                }
                black_box(&m);
            })
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark: iteration
// ---------------------------------------------------------------------------

fn bench_iteration(c: &mut Criterion) {
    let mut group = c.benchmark_group("iteration");

    for size in [1_000, 10_000] {
        let keys: Vec<u64> = (0..size as u64).collect();

        let mut cm = ChainingHashMap::<u64, u64>::with_capacity(size);
        let mut lm = LinearProbingHashMap::<u64, u64>::with_capacity(size);
        let mut rm = RobinHoodHashMap::<u64, u64>::with_capacity(size);
        let mut sm = std::collections::HashMap::with_capacity(size);
        for &k in &keys {
            cm.insert(k, k);
            lm.insert(k, k);
            rm.insert(k, k);
            sm.insert(k, k);
        }

        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(BenchmarkId::new("chaining", size), &(), |b, _| {
            b.iter(|| {
                for (k, v) in cm.iter() {
                    black_box((k, v));
                }
            })
        });

        group.bench_with_input(BenchmarkId::new("linear", size), &(), |b, _| {
            b.iter(|| {
                for (k, v) in lm.iter() {
                    black_box((k, v));
                }
            })
        });

        group.bench_with_input(BenchmarkId::new("robinhood", size), &(), |b, _| {
            b.iter(|| {
                for (k, v) in rm.iter() {
                    black_box((k, v));
                }
            })
        });

        group.bench_with_input(BenchmarkId::new("std_hashmap", size), &(), |b, _| {
            b.iter(|| {
                for (k, v) in sm.iter() {
                    black_box((k, v));
                }
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_sequential_insert,
    bench_random_insert,
    bench_lookup_hit,
    bench_lookup_miss,
    bench_mixed_workload,
    bench_deletion,
    bench_iteration,
);
criterion_main!(benches);
