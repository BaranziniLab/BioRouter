//! Cuckoo filter.
//!
//! A space-efficient approximate membership data structure that supports
//! deletion. Uses fingerprinting with two candidate buckets and
//! kick-out (relocation) on collision.
//!
//! Based on: Fan et al., "Cuckoo Filter: Practically Better Than Bloom" (2014).

use crate::hashing::hash_single;
use serde::{Deserialize, Serialize};
use std::hash::Hash;

/// Maximum number of relocation attempts before giving up.
const MAX_KICKS: usize = 500;

/// Fingerprint size in bits (used to derive the fingerprint mask).
const FINGERPRINT_BITS: u32 = 16;

/// Maximum fingerprint value (non-zero).
const FP_MASK: u64 = (1u64 << FINGERPRINT_BITS) - 1;

/// A non-zero fingerprint. We use 0 as "empty" sentinel.
type Fingerprint = u64;

/// A single bucket holds up to `BUCKET_SIZE` fingerprints.
const BUCKET_SIZE: usize = 4;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Bucket {
    fps: [Fingerprint; BUCKET_SIZE],
    len: u8,
}

impl Bucket {
    fn new() -> Self {
        Bucket {
            fps: [0; BUCKET_SIZE],
            len: 0,
        }
    }

    fn is_full(&self) -> bool {
        self.len as usize >= BUCKET_SIZE
    }

    fn contains(&self, fp: Fingerprint) -> bool {
        self.fps[..self.len as usize].contains(&fp)
    }

    fn insert(&mut self, fp: Fingerprint) -> bool {
        if self.is_full() {
            return false;
        }
        self.fps[self.len as usize] = fp;
        self.len += 1;
        true
    }

    fn remove(&mut self, fp: Fingerprint) -> bool {
        let idx = self.fps[..self.len as usize].iter().position(|&f| f == fp);
        if let Some(i) = idx {
            self.len -= 1;
            self.fps[i] = self.fps[self.len as usize]; // swap-remove
            self.fps[self.len as usize] = 0;
            true
        } else {
            false
        }
    }
}

/// Cuckoo filter supporting insert, lookup, and delete.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CuckooFilter {
    buckets: Vec<Bucket>,
    num_items: u64,
    /// Power-of-two bucket count for fast modulo via masking.
    bucket_mask: u64,
}

impl CuckooFilter {
    /// Create a new Cuckoo filter with capacity for approximately `capacity` items.
    ///
    /// The actual number of buckets is the next power of two >= capacity/BUCKET_SIZE.
    pub fn new(capacity: usize) -> Self {
        let min_buckets = (capacity + BUCKET_SIZE - 1) / BUCKET_SIZE;
        let num_buckets = min_buckets.next_power_of_two().max(2);
        CuckooFilter {
            buckets: vec![Bucket::new(); num_buckets],
            num_items: 0,
            bucket_mask: (num_buckets - 1) as u64,
        }
    }

    /// Derive fingerprint and two bucket indices for an item.
    fn fingerprint_and_buckets<T: Hash + ?Sized>(&self, item: &T) -> (Fingerprint, usize, usize) {
        let hash = hash_single(item);
        let mut fp = (hash >> 32) & FP_MASK;
        // Ensure fingerprint is non-zero
        if fp == 0 {
            fp = 1;
        }
        let i1 = (hash & self.bucket_mask) as usize;
        // Derive i2 from i1 XOR hash of fingerprint
        let fp_hash = hash_single(&fp);
        let i2 = ((i1 as u64) ^ fp_hash) & self.bucket_mask;
        (fp, i1, i2 as usize)
    }

    /// Insert an item. Returns `true` if successfully inserted, `false` if
    /// the filter is full (after MAX_KICKS relocation attempts).
    pub fn insert<T: Hash + ?Sized>(&mut self, item: &T) -> bool {
        let (fp, i1, i2) = self.fingerprint_and_buckets(item);

        // Try direct insertion
        if self.buckets[i1].insert(fp) || self.buckets[i2].insert(fp) {
            self.num_items += 1;
            return true;
        }

        // Both buckets full – start kicking
        let mut current_fp = fp;
        let mut idx = if rand::random::<bool>() { i1 } else { i2 };

        for _ in 0..MAX_KICKS {
            // Evict a random victim
            let victim_pos = (rand::random::<usize>()) % BUCKET_SIZE;
            let victim_fp = self.buckets[idx].fps[victim_pos];
            self.buckets[idx].fps[victim_pos] = current_fp;

            // Compute alternate bucket for the victim
            let fp_hash = hash_single(&victim_fp);
            let alt = ((idx as u64) ^ fp_hash) & self.bucket_mask;

            if self.buckets[alt as usize].insert(victim_fp) {
                self.num_items += 1;
                return true;
            }

            current_fp = victim_fp;
            idx = alt as usize;
        }

        // Failed after MAX_KICKS
        false
    }

    /// Check if an item might be in the filter.
    pub fn contains<T: Hash + ?Sized>(&self, item: &T) -> bool {
        let (fp, i1, i2) = self.fingerprint_and_buckets(item);
        self.buckets[i1].contains(fp) || self.buckets[i2].contains(fp)
    }

    /// Delete an item. Returns `true` if found and removed.
    ///
    /// Only delete items that were actually inserted (otherwise may cause
    /// false negatives for other items sharing the fingerprint).
    pub fn delete<T: Hash + ?Sized>(&mut self, item: &T) -> bool {
        let (fp, i1, i2) = self.fingerprint_and_buckets(item);
        if self.buckets[i1].remove(fp) {
            self.num_items -= 1;
            return true;
        }
        if self.buckets[i2].remove(fp) {
            self.num_items -= 1;
            return true;
        }
        false
    }

    pub fn num_buckets(&self) -> usize {
        self.buckets.len()
    }
    pub fn capacity(&self) -> usize {
        self.buckets.len() * BUCKET_SIZE
    }
    pub fn len(&self) -> u64 {
        self.num_items
    }
    pub fn is_empty(&self) -> bool {
        self.num_items == 0
    }

    /// Approximate load factor.
    pub fn load_factor(&self) -> f64 {
        self.num_items as f64 / self.capacity() as f64
    }

    /// Theoretical FPR for a Cuckoo filter ≈ (load * BUCKET_SIZE) / 2^(fp_bits).
    /// More precisely, ~ 1 - (1 - 1/(2^fp_bits))^(2 * n_buckets * BUCKET_SIZE / n_buckets).
    /// We use the simpler approximation.
    pub fn theoretical_fpr(&self) -> f64 {
        // Per lookup: probability a random fingerprint matches in a bucket of size b
        // is ≈ b / 2^fp_bits. We check two buckets, so:
        // FPR ≈ 1 - (1 - BUCKET_SIZE / 2^fp_bits)^2  ≈ 2*BUCKET_SIZE / 2^fp_bits
        // But for a loaded filter, each bucket might have fewer entries.
        // A more accurate estimate accounts for actual load:
        let avg_fp_per_bucket = if self.buckets.is_empty() {
            0.0
        } else {
            self.num_items as f64 / self.buckets.len() as f64 / 2.0 // 2 candidate buckets per item
        };
        // Probability of fingerprint collision in one bucket check
        let p_one = 1.0 - (1.0 - 1.0 / (FP_MASK as f64 + 1.0)).powf(avg_fp_per_bucket * BUCKET_SIZE as f64);
        1.0 - (1.0 - p_one).powi(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_insert_and_contains() {
        let mut cf = CuckooFilter::new(1000);
        cf.insert(&"hello");
        cf.insert(&"world");
        assert!(cf.contains(&"hello"));
        assert!(cf.contains(&"world"));
        assert!(!cf.contains(&"missing"));
    }

    #[test]
    fn delete_works() {
        let mut cf = CuckooFilter::new(1000);
        cf.insert(&"hello");
        assert!(cf.contains(&"hello"));
        assert!(cf.delete(&"hello"));
        assert!(!cf.contains(&"hello"));
    }

    #[test]
    fn no_false_negatives() {
        let mut cf = CuckooFilter::new(10_000);
        for i in 0..5000u32 {
            assert!(cf.insert(&i), "failed to insert {}", i);
        }
        for i in 0..5000u32 {
            assert!(cf.contains(&i), "false negative for {}", i);
        }
    }

    #[test]
    fn fpr_within_tolerance() {
        let n = 5000;
        let mut cf = CuckooFilter::new(n * 2);
        for i in 0..n {
            cf.insert(&i);
        }
        let measured = crate::analysis::measure_fpr_cuckoo(&cf, n, n);
        // Cuckoo filter FPR is typically very low; allow generous tolerance
        assert!(
            measured < 0.05,
            "measured FPR {} too high for cuckoo filter",
            measured
        );
    }

    #[test]
    fn relocation_under_pressure() {
        // Insert enough items to force some relocations
        let mut cf = CuckooFilter::new(100);
        let mut inserted = 0;
        for i in 0..100u32 {
            if cf.insert(&i) {
                inserted += 1;
            }
        }
        // Most should succeed even in a tight filter
        assert!(inserted >= 80, "only {} of 100 inserted", inserted);
    }

    #[test]
    fn serialization_roundtrip() {
        let mut cf = CuckooFilter::new(1000);
        for i in 0..500u32 {
            cf.insert(&i);
        }
        let json = serde_json::to_string(&cf).unwrap();
        let cf2: CuckooFilter = serde_json::from_str(&json).unwrap();
        for i in 0..500u32 {
            assert!(cf2.contains(&i));
        }
    }
}
