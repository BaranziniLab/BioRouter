//! Counting Bloom filter.
//!
//! Extends the classic Bloom filter by using counters instead of bits,
//! enabling element removal (with caveats about underflow).

use crate::hashing::{BuildMultiHasher, DefaultBuildHasher};
use serde::{Deserialize, Serialize};
use std::hash::Hash;

/// Maximum counter value before saturation (4-bit counters, 0..15).
const MAX_COUNTER: u8 = 15;

/// A Counting Bloom filter that supports removal of elements.
///
/// Each bit position is replaced by a small counter (4 bits).
/// Removal decrements counters; a counter at zero cannot go below zero.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CountingBloomFilter<H: BuildMultiHasher = DefaultBuildHasher> {
    counters: Vec<u8>,
    num_hashes: u32,
    num_items: u64,
    hasher: H,
}

impl CountingBloomFilter<DefaultBuildHasher> {
    /// Create with optimal parameters for expected `n` elements and target `fp_rate`.
    pub fn optimal(n: usize, fp_rate: f64) -> Self {
        assert!(n > 0);
        assert!(fp_rate > 0.0 && fp_rate < 1.0);
        let ln2 = std::f64::consts::LN_2;
        let m = (-(n as f64) * fp_rate.ln() / (ln2 * ln2)).ceil() as usize;
        let k = ((m as f64 / n as f64) * ln2).round().max(1.0) as u32;
        Self::with_params(m.max(1), k, DefaultBuildHasher)
    }
}

impl<H: BuildMultiHasher> CountingBloomFilter<H> {
    /// Create with explicit counter count and hash count.
    pub fn with_params(num_counters: usize, num_hashes: u32, hasher: H) -> Self {
        assert!(num_counters > 0);
        assert!(num_hashes > 0);
        CountingBloomFilter {
            counters: vec![0u8; num_counters],
            num_hashes,
            num_items: 0,
            hasher,
        }
    }

    /// Insert an item, incrementing relevant counters (saturating at MAX_COUNTER).
    pub fn insert<T: Hash + ?Sized>(&mut self, item: &T) {
        let hashes = self.hasher.hash_k(item, self.num_hashes);
        for h in hashes {
            let idx = (h as usize) % self.counters.len();
            if self.counters[idx] < MAX_COUNTER {
                self.counters[idx] += 1;
            }
        }
        self.num_items += 1;
    }

    /// Remove an item, decrementing relevant counters.
    ///
    /// **Warning**: if the item was never inserted, this may cause false
    /// negatives for other items. Only remove items known to have been inserted.
    pub fn remove<T: Hash + ?Sized>(&mut self, item: &T) {
        let hashes = self.hasher.hash_k(item, self.num_hashes);
        for h in hashes {
            let idx = (h as usize) % self.counters.len();
            if self.counters[idx] > 0 {
                self.counters[idx] -= 1;
            }
        }
        if self.num_items > 0 {
            self.num_items -= 1;
        }
    }

    /// Check if an item might be in the set.
    pub fn contains<T: Hash + ?Sized>(&self, item: &T) -> bool {
        let hashes = self.hasher.hash_k(item, self.num_hashes);
        for h in hashes {
            let idx = (h as usize) % self.counters.len();
            if self.counters[idx] == 0 {
                return false;
            }
        }
        true
    }

    pub fn num_counters(&self) -> usize {
        self.counters.len()
    }
    pub fn num_hashes(&self) -> u32 {
        self.num_hashes
    }
    pub fn len(&self) -> u64 {
        self.num_items
    }
    pub fn is_empty(&self) -> bool {
        self.num_items == 0
    }

    /// Theoretical FPR (same formula as standard Bloom).
    pub fn theoretical_fpr(&self) -> f64 {
        let m = self.counters.len() as f64;
        let n = self.num_items as f64;
        let k = self.num_hashes as f64;
        let exp = (-k * n / m).exp();
        (1.0 - exp).powf(k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_false_negatives() {
        let mut cbf = CountingBloomFilter::optimal(1000, 0.01);
        for i in 0..1000u32 {
            cbf.insert(&i);
        }
        for i in 0..1000u32 {
            assert!(cbf.contains(&i));
        }
    }

    #[test]
    fn remove_works() {
        let mut cbf = CountingBloomFilter::with_params(10000, 4, DefaultBuildHasher);
        cbf.insert(&"hello");
        assert!(cbf.contains(&"hello"));
        cbf.remove(&"hello");
        // After removing, it might not be contained (could still be a false positive
        // if bits overlap, but with 10000 slots and 1 item, it should be gone).
        // We test by checking many non-inserted items aren't affected.
        assert!(!cbf.contains(&"hello"));
    }

    #[test]
    fn remove_only_inserted() {
        let mut cbf = CountingBloomFilter::with_params(50000, 4, DefaultBuildHasher);
        for i in 0..100u32 {
            cbf.insert(&i);
        }
        // Remove half
        for i in 0..50u32 {
            cbf.remove(&i);
        }
        // Remaining should still be found
        for i in 50..100u32 {
            assert!(cbf.contains(&i), "false negative for {}", i);
        }
    }

    #[test]
    fn serialization_roundtrip() {
        let mut cbf = CountingBloomFilter::optimal(500, 0.01);
        for i in 0..500u32 {
            cbf.insert(&i);
        }
        let json = serde_json::to_string(&cbf).unwrap();
        let cbf2: CountingBloomFilter = serde_json::from_str(&json).unwrap();
        for i in 0..500u32 {
            assert!(cbf2.contains(&i));
        }
    }
}
