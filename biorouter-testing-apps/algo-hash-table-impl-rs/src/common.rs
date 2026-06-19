//! Common types, traits, and utilities for hash table implementations.
//!
//! This module provides the `HashMap` trait that all implementations must satisfy,
//! a configurable default hasher based on FNV-1a, load-factor configuration, and
//! a collision-heavy hasher for testing.

use std::borrow::Borrow;
use std::hash::{BuildHasher, Hash, Hasher};

// ---------------------------------------------------------------------------
// HashMap trait
// ---------------------------------------------------------------------------

/// Unified interface for all hash-table implementations.
pub trait HashMap<K, V, S = FnvHasherBuilder>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    /// Create an empty map with default capacity (16) and load factor (0.75).
    fn new() -> Self
    where
        Self: Sized;

    /// Create with an explicit initial capacity.
    fn with_capacity(capacity: usize) -> Self
    where
        Self: Sized;

    /// Create with explicit capacity **and** maximum load factor.
    fn with_capacity_and_load_factor(capacity: usize, max_load: f64) -> Self
    where
        Self: Sized;

    /// Insert a key-value pair. Returns the old value if the key was present.
    fn insert(&mut self, key: K, value: V) -> Option<V>;

    /// Get an immutable reference to the value for `key`.
    fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized;

    /// Get a mutable reference to the value for `key`.
    fn get_mut<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized;

    /// Remove a key and return its value.
    fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized;

    /// Number of live entries.
    fn len(&self) -> usize;

    /// Whether the map is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total capacity (bucket count / slot count).
    fn capacity(&self) -> usize;

    /// Current load factor (`len / capacity`).
    fn load_factor(&self) -> f64 {
        if self.capacity() == 0 {
            0.0
        } else {
            self.len() as f64 / self.capacity() as f64
        }
    }

    /// Remove all entries without deallocating.
    fn clear(&mut self);

    /// Whether the map contains `key`.
    fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.get(key).is_some()
    }
}

// ---------------------------------------------------------------------------
// FNV-1a hasher (fast, simple, deterministic within a run)
// ---------------------------------------------------------------------------

/// A fast FNV-1a 64-bit hasher.
#[derive(Clone, Default)]
pub struct FnvHasher(u64);

impl FnvHasher {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x00000100000001b3;
}

impl Hasher for FnvHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// [`BuildHasher`] that produces [`FnvHasher`] instances.
#[derive(Clone, Default)]
pub struct FnvHasherBuilder;

impl BuildHasher for FnvHasherBuilder {
    type Hasher = FnvHasher;

    fn build_hasher(&self) -> FnvHasher {
        FnvHasher(FnvHasher::OFFSET)
    }
}

// ---------------------------------------------------------------------------
// Collision-heavy hasher (for testing cluster behaviour)
// ---------------------------------------------------------------------------

/// A hasher that always returns the same value for **all** keys, forcing
/// maximum collisions. Useful for stress-testing and cluster analysis.
#[derive(Clone, Default)]
pub struct CollisionHasherBuilder;

impl BuildHasher for CollisionHasherBuilder {
    type Hasher = FixedHasher;

    fn build_hasher(&self) -> FixedHasher {
        FixedHasher(0)
    }
}

/// A hasher that always returns 0.
#[derive(Clone)]
pub struct FixedHasher(u64);

impl Hasher for FixedHasher {
    fn write(&mut self, _bytes: &[u8]) {
        // ignore input — always collides
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

/// A hasher that maps every key to one of `N` distinct buckets.
/// This is more realistic than `CollisionHasherBuilder` — it creates
/// moderate collisions rather than total collapse.
#[derive(Clone)]
pub struct ModHasherBuilder {
    pub modulus: u64,
}

impl Default for ModHasherBuilder {
    fn default() -> Self {
        ModHasherBuilder { modulus: 8 }
    }
}

impl BuildHasher for ModHasherBuilder {
    type Hasher = ModHasher;

    fn build_hasher(&self) -> ModHasher {
        ModHasher {
            state: 0,
            modulus: self.modulus,
        }
    }
}

/// A hasher that reduces to `hash % modulus`.
#[derive(Clone)]
pub struct ModHasher {
    state: u64,
    modulus: u64,
}

impl Hasher for ModHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.state = self.state.wrapping_mul(31).wrapping_add(b as u64);
        }
    }
    fn finish(&self) -> u64 {
        self.state % self.modulus.max(1)
    }
}

// ---------------------------------------------------------------------------
// Helper: next power of two >= n
// ---------------------------------------------------------------------------

pub fn next_power_of_two(n: usize) -> usize {
    if n == 0 {
        return 1;
    }
    let mut v = n - 1;
    v |= v >> 1;
    v |= v >> 2;
    v |= v >> 4;
    v |= v >> 8;
    v |= v >> 16;
    if std::mem::size_of::<usize>() > 4 {
        v |= v >> 32;
    }
    v + 1
}

// ---------------------------------------------------------------------------
// Module-level tests for helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_hasher_deterministic() {
        let b = FnvHasherBuilder;
        let mut h1 = b.build_hasher();
        "hello".hash(&mut h1);
        let mut h2 = b.build_hasher();
        "hello".hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());
    }

    #[test]
    fn fnv_hasher_differs_for_different_inputs() {
        let b = FnvHasherBuilder;
        let mut h1 = b.build_hasher();
        "hello".hash(&mut h1);
        let mut h2 = b.build_hasher();
        "world".hash(&mut h2);
        assert_ne!(h1.finish(), h2.finish());
    }

    #[test]
    fn collision_hasher_always_zero() {
        let b = CollisionHasherBuilder;
        let mut h = b.build_hasher();
        "anything".hash(&mut h);
        assert_eq!(h.finish(), 0);
        let mut h2 = b.build_hasher();
        "completely different".hash(&mut h2);
        assert_eq!(h2.finish(), 0);
    }

    #[test]
    fn mod_hasher_respects_modulus() {
        let b = ModHasherBuilder { modulus: 8 };
        for i in 0..100u64 {
            let mut h = b.build_hasher();
            i.hash(&mut h);
            assert!(h.finish() < 8, "hash {} >= modulus 8", h.finish());
        }
    }

    #[test]
    fn next_power_of_two_works() {
        assert_eq!(next_power_of_two(0), 1);
        assert_eq!(next_power_of_two(1), 1);
        assert_eq!(next_power_of_two(2), 2);
        assert_eq!(next_power_of_two(3), 4);
        assert_eq!(next_power_of_two(15), 16);
        assert_eq!(next_power_of_two(16), 16);
        assert_eq!(next_power_of_two(17), 32);
    }
}
