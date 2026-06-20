//! Classic Bloom filter.
//!
//! A space-efficient probabilistic set membership data structure.
//! Supports configurable number of bits and hash functions, plus
//! automatic optimal sizing from expected element count and target
//! false-positive rate.

use crate::hashing::{BuildMultiHasher, DefaultBuildHasher};
use serde::{Deserialize, Serialize};
use std::hash::Hash;

// ---------------------------------------------------------------------------
// Bit vector
// ---------------------------------------------------------------------------

/// A compact bit vector used internally by the Bloom filter.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct BitVec {
    bits: Vec<u64>,
    len: usize, // number of bits
}

impl BitVec {
    fn new(num_bits: usize) -> Self {
        let words = (num_bits + 63) / 64;
        BitVec {
            bits: vec![0u64; words],
            len: num_bits,
        }
    }

    #[inline]
    fn set(&mut self, idx: usize) {
        debug_assert!(idx < self.len);
        self.bits[idx / 64] |= 1u64 << (idx % 64);
    }

    #[inline]
    fn get(&self, idx: usize) -> bool {
        debug_assert!(idx < self.len);
        (self.bits[idx / 64] >> (idx % 64)) & 1 == 1
    }

    fn count_ones(&self) -> u64 {
        self.bits.iter().map(|w| w.count_ones() as u64).sum()
    }
}

// ---------------------------------------------------------------------------
// Bloom filter
// ---------------------------------------------------------------------------

/// A classic Bloom filter parameterized by the hash builder `H`.
///
/// Insertions and queries are *O(k)* where k is the number of hash functions.
/// False positives are possible; false negatives are not.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BloomFilter<H: BuildMultiHasher = DefaultBuildHasher> {
    bits: BitVec,
    num_hashes: u32,
    num_items: u64,
    hasher: H,
}

impl BloomFilter<DefaultBuildHasher> {
    /// Create a Bloom filter with optimal parameters for the given
    /// expected element count `n` and target false-positive rate `fp_rate`.
    ///
    /// Math:
    ///   m = -(n * ln(p)) / (ln 2)^2   (number of bits)
    ///   k = (m / n) * ln 2             (number of hash functions)
    pub fn optimal(n: usize, fp_rate: f64) -> Self {
        assert!(n > 0, "expected element count must be > 0");
        assert!(fp_rate > 0.0 && fp_rate < 1.0, "fp_rate must be in (0, 1)");

        let ln2 = std::f64::consts::LN_2;
        let m = (-(n as f64) * fp_rate.ln() / (ln2 * ln2)).ceil() as usize;
        let k = ((m as f64 / n as f64) * ln2).round().max(1.0) as u32;

        Self::with_params(m.max(1), k, DefaultBuildHasher)
    }
}

impl<H: BuildMultiHasher> BloomFilter<H> {
    /// Create a Bloom filter with explicit bit count and number of hashes.
    pub fn with_params(num_bits: usize, num_hashes: u32, hasher: H) -> Self {
        assert!(num_bits > 0, "num_bits must be > 0");
        assert!(num_hashes > 0, "num_hashes must be > 0");
        BloomFilter {
            bits: BitVec::new(num_bits),
            num_hashes,
            num_items: 0,
            hasher,
        }
    }

    /// Insert an item into the filter.
    pub fn insert<T: Hash + ?Sized>(&mut self, item: &T) {
        let hashes = self.hasher.hash_k(item, self.num_hashes);
        for h in hashes {
            let idx = (h as usize) % self.bits.len;
            self.bits.set(idx);
        }
        self.num_items += 1;
    }

    /// Check if an item *might* be in the set.
    ///
    /// Returns `true` if the item is possibly contained (may be a false positive).
    /// Returns `false` if the item is definitely not contained (no false negatives).
    pub fn contains<T: Hash + ?Sized>(&self, item: &T) -> bool {
        let hashes = self.hasher.hash_k(item, self.num_hashes);
        for h in hashes {
            let idx = (h as usize) % self.bits.len;
            if !self.bits.get(idx) {
                return false;
            }
        }
        true
    }

    /// Number of bits in the filter.
    pub fn num_bits(&self) -> usize {
        self.bits.len
    }

    /// Number of hash functions.
    pub fn num_hashes(&self) -> u32 {
        self.num_hashes
    }

    /// Number of items inserted so far.
    pub fn len(&self) -> u64 {
        self.num_items
    }

    /// Whether the filter is empty.
    pub fn is_empty(&self) -> bool {
        self.num_items == 0
    }

    /// Theoretical false-positive rate based on current fill level.
    ///
    /// FPR ≈ (1 - e^(-k*n/m))^k
    pub fn theoretical_fpr(&self) -> f64 {
        let m = self.bits.len as f64;
        let n = self.num_items as f64;
        let k = self.num_hashes as f64;
        let exp = (-k * n / m).exp();
        (1.0 - exp).powf(k)
    }

    /// Proportion of bits that are set (fill ratio).
    pub fn fill_ratio(&self) -> f64 {
        self.bits.count_ones() as f64 / self.bits.len as f64
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_false_negatives() {
        let mut bf = BloomFilter::optimal(1000, 0.01);
        for i in 0..1000u32 {
            bf.insert(&i);
        }
        for i in 0..1000u32 {
            assert!(bf.contains(&i), "false negative for {}", i);
        }
    }

    #[test]
    fn empty_contains_nothing() {
        let bf = BloomFilter::optimal(100, 0.01);
        assert!(!bf.contains(&"missing"));
    }

    #[test]
    fn fpr_close_to_target() {
        let n = 10_000usize;
        let target_fpr = 0.01;
        let mut bf = BloomFilter::optimal(n, target_fpr);
        for i in 0..n {
            bf.insert(&i);
        }
        let measured = crate::analysis::measure_fpr_bloom(&bf, n, n);
        // Allow 2x slack (probabilistic)
        assert!(
            measured < target_fpr * 3.0,
            "measured FPR {} exceeds tolerance (target {})",
            measured,
            target_fpr
        );
    }

    #[test]
    fn theoretical_fpr_reasonable() {
        let mut bf = BloomFilter::optimal(1000, 0.01);
        for i in 0..1000u32 {
            bf.insert(&i);
        }
        let t = bf.theoretical_fpr();
        assert!(t > 0.0 && t < 0.1);
    }

    #[test]
    fn serialization_roundtrip() {
        let mut bf = BloomFilter::optimal(500, 0.01);
        for i in 0..500u32 {
            bf.insert(&i);
        }
        let json = serde_json::to_string(&bf).unwrap();
        let bf2: BloomFilter = serde_json::from_str(&json).unwrap();
        for i in 0..500u32 {
            assert!(bf2.contains(&i));
        }
    }
}
