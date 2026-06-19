//! CLI demo binary.
//!
//! Inserts a configurable number of entries into each hash-table
//! implementation (and std::collections::HashMap), prints timing and
//! statistics, and runs a quick cluster analysis.

use std::time::Instant;

use algo_hash_table_impl_rs::chaining::ChainingHashMap;
use algo_hash_table_impl_rs::cluster_analysis;
use algo_hash_table_impl_rs::common::HashMap as HashMapTrait;
use algo_hash_table_impl_rs::linear::LinearProbingHashMap;
use algo_hash_table_impl_rs::robinhood::RobinHoodHashMap;

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000);

    println!("╔══════════════════════════════════════════════════════╗");
    println!("║   Hash Table Implementation Comparison              ║");
    println!("║   Entries: {:>8}                                  ║", n);
    println!("╚══════════════════════════════════════════════════════╝");
    println!();

    // ---- Sequential insert benchmark ----
    println!("--- Sequential Insert ---");

    let start = Instant::now();
    {
        let mut m = ChainingHashMap::<u64, u64>::with_capacity(n);
        for i in 0..n as u64 {
            m.insert(i, i.wrapping_mul(7));
        }
        let elapsed = start.elapsed();
        println!(
            "  Chaining:      {:>10.3} ms  (len={}, cap={}, load={:.3})",
            elapsed.as_secs_f64() * 1000.0,
            m.len(),
            m.capacity(),
            m.load_factor(),
        );
    }

    let start = Instant::now();
    {
        let mut m = LinearProbingHashMap::<u64, u64>::with_capacity(n);
        for i in 0..n as u64 {
            m.insert(i, i.wrapping_mul(7));
        }
        let elapsed = start.elapsed();
        println!(
            "  Linear Probing:{:>10.3} ms  (len={}, cap={}, load={:.3}, tombstones={})",
            elapsed.as_secs_f64() * 1000.0,
            m.len(),
            m.capacity(),
            m.load_factor(),
            m.tombstone_count(),
        );
    }

    let start = Instant::now();
    {
        let mut m = RobinHoodHashMap::<u64, u64>::with_capacity(n);
        for i in 0..n as u64 {
            m.insert(i, i.wrapping_mul(7));
        }
        let elapsed = start.elapsed();
        println!(
            "  Robin Hood:    {:>10.3} ms  (len={}, cap={}, load={:.3})",
            elapsed.as_secs_f64() * 1000.0,
            m.len(),
            m.capacity(),
            m.load_factor(),
        );
    }

    let start = Instant::now();
    {
        let mut m = std::collections::HashMap::with_capacity(n);
        for i in 0..n as u64 {
            m.insert(i, i.wrapping_mul(7));
        }
        let elapsed = start.elapsed();
        println!(
            "  std::HashMap:  {:>10.3} ms  (len={})",
            elapsed.as_secs_f64() * 1000.0,
            m.len(),
        );
    }

    // ---- Lookup benchmark ----
    println!("\n--- Lookup (hit) ---");
    let keys: Vec<u64> = (0..n as u64).collect();

    {
        let mut m = ChainingHashMap::<u64, u64>::with_capacity(n);
        for &k in &keys {
            m.insert(k, k);
        }
        let start = Instant::now();
        for &k in &keys {
            std::hint::black_box(m.get(&k));
        }
        let elapsed = start.elapsed();
        println!(
            "  Chaining:      {:>10.3} ms",
            elapsed.as_secs_f64() * 1000.0
        );
    }

    {
        let mut m = LinearProbingHashMap::<u64, u64>::with_capacity(n);
        for &k in &keys {
            m.insert(k, k);
        }
        let start = Instant::now();
        for &k in &keys {
            std::hint::black_box(m.get(&k));
        }
        let elapsed = start.elapsed();
        println!(
            "  Linear Probing:{:>10.3} ms",
            elapsed.as_secs_f64() * 1000.0
        );
    }

    {
        let mut m = RobinHoodHashMap::<u64, u64>::with_capacity(n);
        for &k in &keys {
            m.insert(k, k);
        }
        let start = Instant::now();
        for &k in &keys {
            std::hint::black_box(m.get(&k));
        }
        let elapsed = start.elapsed();
        println!(
            "  Robin Hood:    {:>10.3} ms",
            elapsed.as_secs_f64() * 1000.0
        );
    }

    {
        let mut m = std::collections::HashMap::with_capacity(n);
        for &k in &keys {
            m.insert(k, k);
        }
        let start = Instant::now();
        for &k in &keys {
            std::hint::black_box(m.get(&k));
        }
        let elapsed = start.elapsed();
        println!(
            "  std::HashMap:  {:>10.3} ms",
            elapsed.as_secs_f64() * 1000.0
        );
    }

    // ---- Cluster analysis ----
    println!("\n--- Cluster Analysis (mod-8 hasher, {} entries) ---", n.min(200));
    let reports = cluster_analysis::analyze_all(n.min(200), 8);
    for r in &reports {
        println!("{}", r);
    }

    println!("--- Cluster Analysis (total collision, {} entries) ---", n.min(50));
    let reports = cluster_analysis::analyze_total_collision(n.min(50));
    for r in &reports {
        println!("{}", r);
    }

    println!("\nDone.");
}
