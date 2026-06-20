//! Integration tests for all probabilistic data structures.
//!
//! These tests verify cross-cutting properties:
//! - No false negatives (ever)
//! - FPR within tolerance
//! - Cuckoo eviction/relocation correctness
//! - Serialization round-trip for all types
//! - Property-based tests (randomized inputs)

use algo_bloom_cuckoo_filters_rs::bloom::BloomFilter;
use algo_bloom_cuckoo_filters_rs::counting::CountingBloomFilter;
use algo_bloom_cuckoo_filters_rs::cuckoo::CuckooFilter;
use algo_bloom_cuckoo_filters_rs::scalable::ScalableBloomFilter;
use algo_bloom_cuckoo_filters_rs::analysis::{
    measure_fpr_bloom, measure_fpr_cbf, measure_fpr_cuckoo, measure_fpr_sbf, run_analysis,
};

// =========================================================================
// Property: no false negatives
// =========================================================================

#[test]
fn bloom_no_false_negatives_random_strings() {
    let mut bf = BloomFilter::optimal(5000, 0.001);
    let items: Vec<String> = (0..5000).map(|i| format!("item_{}", i)).collect();
    for item in &items {
        bf.insert(item);
    }
    for item in &items {
        assert!(bf.contains(item), "false negative for {}", item);
    }
}

#[test]
fn counting_no_false_negatives_random_strings() {
    let mut cbf = CountingBloomFilter::optimal(5000, 0.001);
    let items: Vec<String> = (0..5000).map(|i| format!("item_{}", i)).collect();
    for item in &items {
        cbf.insert(item);
    }
    for item in &items {
        assert!(cbf.contains(item), "false negative for {}", item);
    }
}

#[test]
fn cuckoo_no_false_negatives_random_strings() {
    let mut cf = CuckooFilter::new(20_000);
    let items: Vec<String> = (0..5000).map(|i| format!("item_{}", i)).collect();
    for item in &items {
        cf.insert(item);
    }
    for item in &items {
        assert!(cf.contains(item), "false negative for {}", item);
    }
}

#[test]
fn scalable_no_false_negatives_random_strings() {
    let mut sbf = ScalableBloomFilter::new(200, 0.001);
    let items: Vec<String> = (0..2000).map(|i| format!("item_{}", i)).collect();
    for item in &items {
        sbf.insert(item);
    }
    for item in &items {
        assert!(sbf.contains(item), "false negative for {}", item);
    }
}

// =========================================================================
// Property: FPR within tolerance
// =========================================================================

#[test]
fn bloom_fpr_within_tolerance() {
    for &(n, target_fpr) in &[(1000, 0.1), (5000, 0.01), (10000, 0.001)] {
        let mut bf = BloomFilter::optimal(n, target_fpr);
        for i in 0..n {
            bf.insert(&(i as u64));
        }
        let measured = measure_fpr_bloom(&bf, n, n);
        assert!(
            measured < target_fpr * 3.0,
            "Bloom n={} target_fpr={} measured={}",
            n,
            target_fpr,
            measured
        );
    }
}

#[test]
fn counting_fpr_within_tolerance() {
    let n = 5000;
    let target_fpr = 0.01;
    let mut cbf = CountingBloomFilter::optimal(n, target_fpr);
    for i in 0..n {
        cbf.insert(&(i as u64));
    }
    let measured = measure_fpr_cbf(&cbf, n, n);
    assert!(
        measured < target_fpr * 3.0,
        "CountingBloom measured FPR {} exceeds tolerance",
        measured
    );
}

#[test]
fn cuckoo_fpr_within_tolerance() {
    let n = 5000;
    let mut cf = CuckooFilter::new(n * 2);
    for i in 0..n {
        cf.insert(&(i as u64));
    }
    let measured = measure_fpr_cuckoo(&cf, n, n);
    assert!(
        measured < 0.05,
        "Cuckoo measured FPR {} too high",
        measured
    );
}

#[test]
fn scalable_fpr_within_tolerance() {
    let n = 3000;
    let target_fpr = 0.01;
    let mut sbf = ScalableBloomFilter::new(300, target_fpr);
    for i in 0..n {
        sbf.insert(&(i as u64));
    }
    let measured = measure_fpr_sbf(&sbf, n, n);
    assert!(
        measured < target_fpr * 5.0,
        "ScalableBloom measured FPR {} exceeds tolerance (target {})",
        measured,
        target_fpr
    );
}

// =========================================================================
// Cuckoo: eviction and relocation correctness
// =========================================================================

#[test]
fn cuckoo_eviction_preserves_existing() {
    // Fill a small filter to force evictions, verify all inserted items are found
    let mut cf = CuckooFilter::new(200);
    let mut inserted = Vec::new();
    for i in 0..200u32 {
        if cf.insert(&i) {
            inserted.push(i);
        }
    }
    // All successfully inserted items should still be found
    for &i in &inserted {
        assert!(cf.contains(&i), "lost item {} after eviction", i);
    }
}

#[test]
fn cuckoo_delete_and_reinsert() {
    let mut cf = CuckooFilter::new(1000);
    for i in 0..500u32 {
        cf.insert(&i);
    }
    // Delete half
    for i in 0..250u32 {
        assert!(cf.delete(&i), "failed to delete {}", i);
    }
    // Reinsert
    for i in 0..250u32 {
        assert!(cf.insert(&i), "failed to reinsert {}", i);
    }
    // All should be present
    for i in 0..500u32 {
        assert!(cf.contains(&i), "missing after delete+reinsert: {}", i);
    }
}

#[test]
fn cuckoo_high_load_factor() {
    let capacity = 500;
    let mut cf = CuckooFilter::new(capacity);
    let mut count = 0;
    for i in 0..capacity * 2 {
        if cf.insert(&(i as u64)) {
            count += 1;
        }
    }
    // Should insert a good fraction even near capacity
    assert!(
        count >= capacity * 80 / 100,
        "only inserted {} / {} items",
        count,
        capacity
    );
    println!("Cuckoo high load: inserted {}/{} (load {:.2})", count, capacity, cf.load_factor());
}

// =========================================================================
// Serialization round-trip
// =========================================================================

#[test]
fn bloom_serialization_roundtrip() {
    let mut bf = BloomFilter::optimal(1000, 0.01);
    for i in 0..1000u32 {
        bf.insert(&i);
    }
    let json = serde_json::to_string(&bf).unwrap();
    let bf2: BloomFilter = serde_json::from_str(&json).unwrap();
    for i in 0..1000u32 {
        assert!(bf2.contains(&i), "roundtrip: lost {}", i);
    }
    // Absent items should still be absent (or FP) — verify structure preserved
    assert_eq!(bf.num_bits(), bf2.num_bits());
    assert_eq!(bf.num_hashes(), bf2.num_hashes());
}

#[test]
fn counting_serialization_roundtrip() {
    let mut cbf = CountingBloomFilter::optimal(1000, 0.01);
    for i in 0..1000u32 {
        cbf.insert(&i);
    }
    let json = serde_json::to_string(&cbf).unwrap();
    let cbf2: CountingBloomFilter = serde_json::from_str(&json).unwrap();
    for i in 0..1000u32 {
        assert!(cbf2.contains(&i));
    }
}

#[test]
fn cuckoo_serialization_roundtrip() {
    let mut cf = CuckooFilter::new(5000);
    for i in 0..2000u32 {
        cf.insert(&i);
    }
    let json = serde_json::to_string(&cf).unwrap();
    let cf2: CuckooFilter = serde_json::from_str(&json).unwrap();
    for i in 0..2000u32 {
        assert!(cf2.contains(&i));
    }
}

#[test]
fn scalable_serialization_roundtrip() {
    let mut sbf = ScalableBloomFilter::new(200, 0.01);
    for i in 0..1000u32 {
        sbf.insert(&i);
    }
    let json = serde_json::to_string(&sbf).unwrap();
    let sbf2: ScalableBloomFilter = serde_json::from_str(&json).unwrap();
    for i in 0..1000u32 {
        assert!(sbf2.contains(&i));
    }
}

// =========================================================================
// Analysis module
// =========================================================================

#[test]
fn analysis_run_analysis_smoke() {
    let results = run_analysis(2000, 0.01);
    assert_eq!(results.len(), 4);
    for r in &results {
        assert!(r.measured_fpr >= 0.0);
        assert!(r.theoretical_fpr >= 0.0);
        assert!(r.bits_per_element > 0.0);
    }
}

// =========================================================================
// Edge cases
// =========================================================================

#[test]
fn bloom_single_item() {
    let mut bf = BloomFilter::optimal(1, 0.01);
    bf.insert(&42u32);
    assert!(bf.contains(&42u32));
    assert_eq!(bf.len(), 1);
}

#[test]
fn cuckoo_single_item() {
    let mut cf = CuckooFilter::new(4);
    cf.insert(&42u32);
    assert!(cf.contains(&42u32));
    assert_eq!(cf.len(), 1);
}

#[test]
fn scalable_single_item() {
    let mut sbf = ScalableBloomFilter::new(1, 0.01);
    sbf.insert(&42u32);
    assert!(sbf.contains(&42u32));
}

#[test]
fn counting_insert_remove_cycle() {
    let mut cbf = CountingBloomFilter::optimal(100, 0.01);
    for cycle in 0..10 {
        for i in 0..100u32 {
            cbf.insert(&(i + cycle * 100));
        }
        for i in 0..50u32 {
            cbf.remove(&(i + cycle * 100));
        }
    }
    assert_eq!(cbf.len(), 500);
}

// =========================================================================
// Property: mixed types work generically
// =========================================================================

#[test]
fn works_with_different_types() {
    let mut bf = BloomFilter::optimal(100, 0.01);
    bf.insert(&42u32);
    bf.insert(&"hello");
    bf.insert(&3.14f64.to_bits());
    bf.insert(&vec![1, 2, 3]);
    assert!(bf.contains(&42u32));
    assert!(bf.contains(&"hello"));
    assert!(bf.contains(&3.14f64.to_bits()));
    assert!(bf.contains(&vec![1, 2, 3]));
}

#[test]
fn works_with_bytes() {
    let mut bf = BloomFilter::optimal(100, 0.01);
    let data: &[u8] = b"binary data here";
    bf.insert(data);
    assert!(bf.contains(data));
}
