//! CLI demo for the probabilistic data structures library.
//!
//! Demonstrates Bloom, Counting Bloom, Cuckoo, and Scalable Bloom filters
//! with insert/query operations, FPR measurement, and benchmarking.

use algo_bloom_cuckoo_filters_rs::analysis::{run_analysis, run_benchmark};
use algo_bloom_cuckoo_filters_rs::bloom::BloomFilter;
use algo_bloom_cuckoo_filters_rs::counting::CountingBloomFilter;
use algo_bloom_cuckoo_filters_rs::cuckoo::CuckooFilter;
use algo_bloom_cuckoo_filters_rs::scalable::ScalableBloomFilter;

fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║   Probabilistic Data Structures — Rust Library Demo         ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    demo_bloom();
    demo_counting_bloom();
    demo_cuckoo();
    demo_scalable();
    demo_fpr_analysis();
    demo_benchmark();
}

fn demo_bloom() {
    println!("── Bloom Filter ──────────────────────────────────────────────");
    let n = 10_000;
    let target_fpr = 0.01;
    let mut bf = BloomFilter::optimal(n, target_fpr);

    println!("  Created: {} bits, {} hash functions", bf.num_bits(), bf.num_hashes());

    // Insert
    for i in 0..n {
        bf.insert(&i);
    }
    println!("  Inserted {} items. Fill ratio: {:.4}", bf.len(), bf.fill_ratio());

    // Query
    let mut found = 0;
    for i in 0..n {
        if bf.contains(&i) {
            found += 1;
        }
    }
    println!("  Query inserted items: {}/{} found (no false negatives)", found, n);

    // Check absent items
    let mut fp = 0;
    let test_count = n;
    for i in n..(n + test_count) {
        if bf.contains(&i) {
            fp += 1;
        }
    }
    println!(
        "  Measured FPR: {:.6}  (target: {:.4}, theoretical: {:.6})",
        fp as f64 / test_count as f64,
        target_fpr,
        bf.theoretical_fpr()
    );
    println!();
}

fn demo_counting_bloom() {
    println!("── Counting Bloom Filter ─────────────────────────────────────");
    let n = 10_000;
    let mut cbf = CountingBloomFilter::optimal(n, 0.01);

    println!(
        "  Created: {} counters (4-bit), {} hash functions",
        cbf.num_counters(),
        cbf.num_hashes()
    );

    for i in 0..n {
        cbf.insert(&i);
    }
    println!("  Inserted {} items", cbf.len());

    // Remove half
    for i in 0..(n / 2) {
        cbf.remove(&i);
    }
    println!("  Removed {} items", n / 2);

    // Check remaining
    let mut still_found = 0;
    for i in (n / 2)..n {
        if cbf.contains(&i) {
            still_found += 1;
        }
    }
    println!("  Remaining items found: {}/{}", still_found, n / 2);
    println!();
}

fn demo_cuckoo() {
    println!("── Cuckoo Filter ────────────────────────────────────────────");
    let n = 10_000;
    let mut cf = CuckooFilter::new(n * 2);

    println!("  Created: {} buckets, capacity {}", cf.num_buckets(), cf.capacity());

    let mut inserted = 0;
    for i in 0..n {
        if cf.insert(&i) {
            inserted += 1;
        }
    }
    println!(
        "  Inserted {}/{} items (load factor: {:.3})",
        inserted,
        n,
        cf.load_factor()
    );

    // Delete some
    let mut deleted = 0;
    for i in 0..(n / 4) {
        if cf.delete(&i) {
            deleted += 1;
        }
    }
    println!("  Deleted {} items", deleted);

    // Check remaining
    let mut found = 0;
    for i in (n / 4)..n {
        if cf.contains(&i) {
            found += 1;
        }
    }
    println!("  Query remaining: {}/{} found", found, n - n / 4);

    let mut fp = 0;
    for i in n..(n * 2) {
        if cf.contains(&i) {
            fp += 1;
        }
    }
    println!(
        "  Measured FPR: {:.6}  (theoretical: {:.6})",
        fp as f64 / n as f64,
        cf.theoretical_fpr()
    );
    println!();
}

fn demo_scalable() {
    println!("── Scalable Bloom Filter ─────────────────────────────────────");
    let mut sbf = ScalableBloomFilter::new(100, 0.01);

    println!("  Created with initial capacity 100, target FPR 0.01");

    for i in 0..5000 {
        sbf.insert(&i);
    }
    println!(
        "  Inserted {} items across {} layers (total bits: {})",
        sbf.len(),
        sbf.num_layers(),
        sbf.total_bits()
    );

    let mut found = 0;
    for i in 0..5000 {
        if sbf.contains(&i) {
            found += 1;
        }
    }
    println!("  Query: {}/{} found (no false negatives)", found, 5000);
    println!();
}

fn demo_fpr_analysis() {
    println!("── FPR Analysis ─────────────────────────────────────────────");
    let n = 5000;
    let target_fpr = 0.01;
    let results = run_analysis(n, target_fpr);
    for r in &results {
        println!("  {}", r);
    }
    println!();
}

fn demo_benchmark() {
    println!("── Benchmark ────────────────────────────────────────────────");
    let n = 50_000;
    let target_fpr = 0.01;
    let (benchmarks, fpr_results) = run_benchmark(n, target_fpr);

    println!("  Throughput:");
    for b in &benchmarks {
        println!("    {}", b);
    }
    println!();
    println!("  FPR at n={}, target={}:", n, target_fpr);
    for r in &fpr_results {
        println!("    {}", r);
    }
    println!();
}
