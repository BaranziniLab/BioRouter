//! Empirical false-positive rate analysis utilities.
//!
//! Provides functions to measure actual FPR by inserting known items and
//! then querying items known to be absent.

use crate::bloom::BloomFilter;
use crate::counting::CountingBloomFilter;
use crate::cuckoo::CuckooFilter;
use crate::scalable::ScalableBloomFilter;

/// Measure FPR for a Bloom filter.
///
/// Inserts `n_insert` items (0..n_insert), then queries `n_query` items
/// known to be absent (n_insert..n_insert+n_query) and returns the
/// fraction that returned `true`.
pub fn measure_fpr_bloom(bf: &BloomFilter, n_insert: usize, n_query: usize) -> f64 {
    // The bf already has items inserted; we just query absent items.
    let start = n_insert as u64;
    let end = start + n_query as u64;
    let mut false_positives = 0u64;
    for i in start..end {
        if bf.contains(&i) {
            false_positives += 1;
        }
    }
    false_positives as f64 / n_query as f64
}

/// Measure FPR for a Counting Bloom filter.
pub fn measure_fpr_cbf(cbf: &CountingBloomFilter, n_insert: usize, n_query: usize) -> f64 {
    let start = n_insert as u64;
    let end = start + n_query as u64;
    let mut false_positives = 0u64;
    for i in start..end {
        if cbf.contains(&i) {
            false_positives += 1;
        }
    }
    false_positives as f64 / n_query as f64
}

/// Measure FPR for a Cuckoo filter.
pub fn measure_fpr_cuckoo(cf: &CuckooFilter, n_insert: usize, n_query: usize) -> f64 {
    let start = n_insert as u64;
    let end = start + n_query as u64;
    let mut false_positives = 0u64;
    for i in start..end {
        if cf.contains(&i) {
            false_positives += 1;
        }
    }
    false_positives as f64 / n_query as f64
}

/// Measure FPR for a Scalable Bloom filter.
pub fn measure_fpr_sbf(sbf: &ScalableBloomFilter, n_insert: usize, n_query: usize) -> f64 {
    let start = n_insert as u64;
    let end = start + n_query as u64;
    let mut false_positives = 0u64;
    for i in start..end {
        if sbf.contains(&i) {
            false_positives += 1;
        }
    }
    false_positives as f64 / n_query as f64
}

/// Result of an FPR analysis run.
#[derive(Debug, Clone)]
pub struct FprResult {
    pub structure: String,
    pub items_inserted: usize,
    pub queries_tested: usize,
    pub false_positives: u64,
    pub measured_fpr: f64,
    pub theoretical_fpr: f64,
    pub bits_per_element: f64,
}

impl std::fmt::Display for FprResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:<25} n={:<8} queries={:<8} FP={:<6} measured_FPR={:.6}  theoretical_FPR={:.6}  bits/elem={:.1}",
            self.structure,
            self.items_inserted,
            self.queries_tested,
            self.false_positives,
            self.measured_fpr,
            self.theoretical_fpr,
            self.bits_per_element
        )
    }
}

/// Run a comprehensive FPR analysis across all filter types with
/// the given parameters and return a vector of results.
pub fn run_analysis(n: usize, target_fpr: f64) -> Vec<FprResult> {
    let query_count = n;
    let mut results = Vec::new();

    // -- Bloom filter --
    {
        let mut bf = BloomFilter::optimal(n, target_fpr);
        for i in 0..n {
            bf.insert(&(i as u64));
        }
        let measured = measure_fpr_bloom(&bf, n, query_count);
        results.push(FprResult {
            structure: "BloomFilter".to_string(),
            items_inserted: n,
            queries_tested: query_count,
            false_positives: (measured * query_count as f64) as u64,
            measured_fpr: measured,
            theoretical_fpr: bf.theoretical_fpr(),
            bits_per_element: bf.num_bits() as f64 / n as f64,
        });
    }

    // -- Counting Bloom filter --
    {
        let mut cbf = CountingBloomFilter::optimal(n, target_fpr);
        for i in 0..n {
            cbf.insert(&(i as u64));
        }
        let measured = measure_fpr_cbf(&cbf, n, query_count);
        results.push(FprResult {
            structure: "CountingBloomFilter".to_string(),
            items_inserted: n,
            queries_tested: query_count,
            false_positives: (measured * query_count as f64) as u64,
            measured_fpr: measured,
            theoretical_fpr: cbf.theoretical_fpr(),
            bits_per_element: cbf.num_counters() as f64 * 4.0 / n as f64, // 4-bit counters
        });
    }

    // -- Cuckoo filter --
    {
        let mut cf = CuckooFilter::new(n * 2);
        let mut inserted = 0;
        for i in 0..n {
            if cf.insert(&(i as u64)) {
                inserted += 1;
            }
        }
        let measured = measure_fpr_cuckoo(&cf, n, query_count);
        results.push(FprResult {
            structure: "CuckooFilter".to_string(),
            items_inserted: inserted,
            queries_tested: query_count,
            false_positives: (measured * query_count as f64) as u64,
            measured_fpr: measured,
            theoretical_fpr: cf.theoretical_fpr(),
            bits_per_element: cf.capacity() as f64 * 16.0 / n as f64, // 16-bit fingerprints, 4 per bucket
        });
    }

    // -- Scalable Bloom filter --
    {
        let mut sbf = ScalableBloomFilter::new(n / 10 + 1, target_fpr);
        for i in 0..n {
            sbf.insert(&(i as u64));
        }
        let measured = measure_fpr_sbf(&sbf, n, query_count);
        results.push(FprResult {
            structure: "ScalableBloomFilter".to_string(),
            items_inserted: n,
            queries_tested: query_count,
            false_positives: (measured * query_count as f64) as u64,
            measured_fpr: measured,
            theoretical_fpr: sbf.theoretical_fpr(),
            bits_per_element: sbf.total_bits() as f64 / n as f64,
        });
    }

    results
}

/// Benchmark result for throughput measurement.
#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub structure: String,
    pub operation: String, // "insert" or "query"
    pub items: usize,
    pub elapsed_ns: u128,
    pub ops_per_sec: f64,
}

impl std::fmt::Display for BenchmarkResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:<25} {:<8} {:<10} ops/sec={:.0}",
            self.structure, self.operation, self.items, self.ops_per_sec
        )
    }
}

/// Run a comprehensive benchmark: insert and query throughput + measured FPR.
pub fn run_benchmark(n: usize, target_fpr: f64) -> (Vec<BenchmarkResult>, Vec<FprResult>) {
    use std::time::Instant;

    let mut benchmarks = Vec::new();

    // -- Bloom --
    {
        let mut bf = BloomFilter::optimal(n, target_fpr);
        let start = Instant::now();
        for i in 0..n {
            bf.insert(&(i as u64));
        }
        let elapsed = start.elapsed().as_nanos();
        benchmarks.push(BenchmarkResult {
            structure: "BloomFilter".into(),
            operation: "insert".into(),
            items: n,
            elapsed_ns: elapsed,
            ops_per_sec: n as f64 / (elapsed as f64 / 1e9),
        });

        let start = Instant::now();
        for i in 0..n {
            bf.contains(&(i as u64));
        }
        let elapsed = start.elapsed().as_nanos();
        benchmarks.push(BenchmarkResult {
            structure: "BloomFilter".into(),
            operation: "query".into(),
            items: n,
            elapsed_ns: elapsed,
            ops_per_sec: n as f64 / (elapsed as f64 / 1e9),
        });
    }

    // -- Counting Bloom --
    {
        let mut cbf = CountingBloomFilter::optimal(n, target_fpr);
        let start = Instant::now();
        for i in 0..n {
            cbf.insert(&(i as u64));
        }
        let elapsed = start.elapsed().as_nanos();
        benchmarks.push(BenchmarkResult {
            structure: "CountingBloomFilter".into(),
            operation: "insert".into(),
            items: n,
            elapsed_ns: elapsed,
            ops_per_sec: n as f64 / (elapsed as f64 / 1e9),
        });

        let start = Instant::now();
        for i in 0..n {
            cbf.contains(&(i as u64));
        }
        let elapsed = start.elapsed().as_nanos();
        benchmarks.push(BenchmarkResult {
            structure: "CountingBloomFilter".into(),
            operation: "query".into(),
            items: n,
            elapsed_ns: elapsed,
            ops_per_sec: n as f64 / (elapsed as f64 / 1e9),
        });
    }

    // -- Cuckoo --
    {
        let mut cf = CuckooFilter::new(n * 2);
        let start = Instant::now();
        for i in 0..n {
            cf.insert(&(i as u64));
        }
        let elapsed = start.elapsed().as_nanos();
        benchmarks.push(BenchmarkResult {
            structure: "CuckooFilter".into(),
            operation: "insert".into(),
            items: n,
            elapsed_ns: elapsed,
            ops_per_sec: n as f64 / (elapsed as f64 / 1e9),
        });

        let start = Instant::now();
        for i in 0..n {
            cf.contains(&(i as u64));
        }
        let elapsed = start.elapsed().as_nanos();
        benchmarks.push(BenchmarkResult {
            structure: "CuckooFilter".into(),
            operation: "query".into(),
            items: n,
            elapsed_ns: elapsed,
            ops_per_sec: n as f64 / (elapsed as f64 / 1e9),
        });
    }

    // -- Scalable Bloom --
    {
        let mut sbf = ScalableBloomFilter::new(n / 10 + 1, target_fpr);
        let start = Instant::now();
        for i in 0..n {
            sbf.insert(&(i as u64));
        }
        let elapsed = start.elapsed().as_nanos();
        benchmarks.push(BenchmarkResult {
            structure: "ScalableBloomFilter".into(),
            operation: "insert".into(),
            items: n,
            elapsed_ns: elapsed,
            ops_per_sec: n as f64 / (elapsed as f64 / 1e9),
        });

        let start = Instant::now();
        for i in 0..n {
            sbf.contains(&(i as u64));
        }
        let elapsed = start.elapsed().as_nanos();
        benchmarks.push(BenchmarkResult {
            structure: "ScalableBloomFilter".into(),
            operation: "query".into(),
            items: n,
            elapsed_ns: elapsed,
            ops_per_sec: n as f64 / (elapsed as f64 / 1e9),
        });
    }

    let fpr_results = run_analysis(n, target_fpr);
    (benchmarks, fpr_results)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_returns_results_for_all_structures() {
        let results = run_analysis(1000, 0.01);
        assert_eq!(results.len(), 4);
        assert_eq!(results[0].structure, "BloomFilter");
        assert_eq!(results[1].structure, "CountingBloomFilter");
        assert_eq!(results[2].structure, "CuckooFilter");
        assert_eq!(results[3].structure, "ScalableBloomFilter");
    }

    #[test]
    fn benchmark_returns_throughput() {
        let (benchmarks, _) = run_benchmark(5000, 0.01);
        assert_eq!(benchmarks.len(), 8); // 4 structures × 2 ops
        for b in &benchmarks {
            assert!(b.ops_per_sec > 0.0);
        }
    }

    #[test]
    fn measured_fpr_nonnegative() {
        let results = run_analysis(2000, 0.01);
        for r in &results {
            assert!(r.measured_fpr >= 0.0);
            assert!(r.measured_fpr <= 1.0);
        }
    }

    #[test]
    fn display_works() {
        let results = run_analysis(500, 0.05);
        for r in &results {
            let s = format!("{}", r);
            assert!(s.contains("measured_FPR"));
        }
    }
}
