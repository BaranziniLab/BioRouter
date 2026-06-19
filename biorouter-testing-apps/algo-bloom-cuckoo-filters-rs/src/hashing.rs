//! Pluggable hashing infrastructure for probabilistic filters.
//!
//! Provides a `BuildHasher`-like trait that produces multiple independent
//! hash values from a single item, which is the common interface needed
//! by Bloom-family and Cuckoo filters.

use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

// ---------------------------------------------------------------------------
// DoubleHasher – produces k independent hash values via double-hashing
// ---------------------------------------------------------------------------

/// A hasher that derives k independent hash values from two base hashes
/// using the formula: h_i(x) = h1(x) + i * h2(x) (modulo 2^64).
///
/// This is the "enhanced double hashing" technique from Kirsch & Mitzenmacher
/// (2006), which is nearly as good as fully independent hashing for Bloom
/// filters and Cuckoo filters.
#[derive(Clone, Debug)]
pub struct DoubleHasher {
    h1: u64,
    h2: u64,
    k: u32,
    i: u32,
}

impl DoubleHasher {
    /// Create a new DoubleHasher for `item` that will produce `k` hashes.
    pub fn new<T: Hash + ?Sized>(item: &T, k: u32) -> Self {
        let mut s1 = DefaultHasher::new();
        item.hash(&mut s1);
        let h1 = s1.finish();

        let mut s2 = DefaultHasher::new();
        0xDEAD_BEEF_CAFE_BABEu64.hash(&mut s2);
        item.hash(&mut s2);
        let h2 = s2.finish();

        DoubleHasher { h1, h2, k, i: 0 }
    }

    /// Consume all remaining hash values and return them as a Vec.
    pub fn collect(mut self) -> Vec<u64> {
        let mut out = Vec::with_capacity(self.k as usize);
        while let Some(v) = self.next() {
            out.push(v);
        }
        out
    }
}

impl Iterator for DoubleHasher {
    type Item = u64;

    fn next(&mut self) -> Option<u64> {
        if self.i >= self.k {
            return None;
        }
        // h_i = h1 + i * h2  (wrapping)
        let val = self.h1.wrapping_add((self.i as u64).wrapping_mul(self.h2));
        self.i += 1;
        Some(val)
    }
}

// ---------------------------------------------------------------------------
// DefaultBuildHasher – a simple wrapper that can be used as a trait-object
// style pluggable hasher for filters.
// ---------------------------------------------------------------------------

/// Trait for building a stream of k hash values for a given item.
pub trait BuildMultiHasher {
    /// Produce `k` hash values for `item`.
    fn hash_k<T: Hash + ?Sized>(&self, item: &T, k: u32) -> Vec<u64>;
}

/// The default multi-hash builder using enhanced double hashing.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DefaultBuildHasher;

impl BuildMultiHasher for DefaultBuildHasher {
    fn hash_k<T: Hash + ?Sized>(&self, item: &T, k: u32) -> Vec<u64> {
        DoubleHasher::new(item, k).collect()
    }
}

// ---------------------------------------------------------------------------
// Convenience: produce a single u64 hash for an item (used by Cuckoo filter)
// ---------------------------------------------------------------------------

/// Return a single 64-bit hash of `item`.
pub fn hash_single<T: Hash + ?Sized>(item: &T) -> u64 {
    let mut s = DefaultHasher::new();
    item.hash(&mut s);
    s.finish()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_hasher_produces_k_values() {
        let h = DoubleHasher::new(&"hello", 5);
        let vals: Vec<u64> = h.collect();
        assert_eq!(vals.len(), 5);
    }

    #[test]
    fn double_hasher_deterministic() {
        let a = DoubleHasher::new(&42u64, 3).collect();
        let b = DoubleHasher::new(&42u64, 3).collect();
        assert_eq!(a, b);
    }

    #[test]
    fn double_hasher_different_items_differ() {
        let a = DoubleHasher::new(&"alpha", 4).collect();
        let b = DoubleHasher::new(&"beta", 4).collect();
        // Extremely unlikely to be all-equal
        assert_ne!(a, b);
    }

    #[test]
    fn default_build_hasher_works() {
        let bh = DefaultBuildHasher;
        let h = bh.hash_k(&"test", 3);
        assert_eq!(h.len(), 3);
    }

    #[test]
    fn hash_single_deterministic() {
        assert_eq!(hash_single(&"foo"), hash_single(&"foo"));
    }
}
