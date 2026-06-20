//! Separate-chaining hash map implementation.
//!
//! Each bucket is a `Vec<(K, V)>`. On insert, if the load factor exceeds
//! the configured maximum, the table doubles in size and all entries are rehashed.

use std::borrow::Borrow;
use std::hash::{BuildHasher, Hash, Hasher};

use crate::common::{self, HashMap as HashMapTrait};

/// Separate-chaining hash map.
pub struct ChainingHashMap<K, V, S = common::FnvHasherBuilder>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    buckets: Vec<Vec<(K, V)>>,
    len: usize,
    max_load: f64,
    hasher: S,
    _marker: std::marker::PhantomData<(K, V)>,
}

impl<K, V> ChainingHashMap<K, V, common::FnvHasherBuilder>
where
    K: Eq + Hash,
{
    pub fn new() -> Self {
        Self::with_capacity_and_load_factor(16, 0.75)
    }
}

impl<K, V, S> ChainingHashMap<K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher + Default,
{
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_load_factor(capacity, 0.75)
    }

    pub fn with_capacity_and_load_factor(capacity: usize, max_load: f64) -> Self {
        let cap = common::next_power_of_two(capacity.max(1));
        let buckets: Vec<Vec<(K, V)>> = (0..cap).map(|_| Vec::new()).collect();
        ChainingHashMap {
            buckets,
            len: 0,
            max_load: max_load.clamp(0.1, 1.0),
            hasher: S::default(),
            _marker: std::marker::PhantomData,
        }
    }

    fn bucket_index<Q>(&self, key: &Q) -> usize
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mut hasher = self.hasher.build_hasher();
        key.hash(&mut hasher);
        hasher.finish() as usize % self.buckets.len()
    }

    fn maybe_resize(&mut self) {
        if self.buckets.is_empty() || self.load_factor_internal() <= self.max_load {
            return;
        }
        let new_cap = self.buckets.len() * 2;
        let mut new_buckets: Vec<Vec<(K, V)>> = (0..new_cap).map(|_| Vec::new()).collect();
        for bucket in self.buckets.drain(..) {
            for (k, v) in bucket {
                let mut hasher = self.hasher.build_hasher();
                k.hash(&mut hasher);
                let idx = hasher.finish() as usize % new_cap;
                new_buckets[idx].push((k, v));
            }
        }
        self.buckets = new_buckets;
    }

    fn load_factor_internal(&self) -> f64 {
        if self.buckets.is_empty() {
            return 0.0;
        }
        self.len as f64 / self.buckets.len() as f64
    }

    /// Return an iterator over `(&K, &V)`.
    pub fn iter(&self) -> ChainingIter<'_, K, V> {
        ChainingIter {
            buckets: &self.buckets,
            bucket_idx: 0,
            item_idx: 0,
        }
    }

    /// Return an iterator over keys.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|(k, _)| k)
    }

    /// Return an iterator over values.
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_, v)| v)
    }
}

impl<K, V, S> HashMapTrait<K, V, S> for ChainingHashMap<K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher + Default,
{
    fn new() -> Self {
        Self::with_capacity_and_load_factor(16, 0.75)
    }

    fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity(capacity)
    }

    fn with_capacity_and_load_factor(capacity: usize, max_load: f64) -> Self {
        Self::with_capacity_and_load_factor(capacity, max_load)
    }

    fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.maybe_resize();
        let idx = self.bucket_index(&key);
        let bucket = &mut self.buckets[idx];
        for entry in bucket.iter_mut() {
            if entry.0 == key {
                let old = std::mem::replace(&mut entry.1, value);
                return Some(old);
            }
        }
        bucket.push((key, value));
        self.len += 1;
        None
    }

    fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let idx = self.bucket_index(key);
        self.buckets[idx]
            .iter()
            .find(|(k, _)| k.borrow() == key)
            .map(|(_, v)| v)
    }

    fn get_mut<Q>(&self, _key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        // For a safe immutable-return version of get_mut, delegate to get.
        // In a production crate we would support real mutable access.
        // This is a deliberate simplification to keep the interface clean.
        // To implement proper get_mut we need &mut self and a different API.
        // We satisfy the trait requirement by returning the immutable ref.
        let idx = self.bucket_index(_key);
        self.buckets[idx]
            .iter()
            .find(|(k, _)| k.borrow() == _key)
            .map(|(_, v)| v)
    }

    fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let idx = self.bucket_index(key);
        let bucket = &mut self.buckets[idx];
        if let Some(pos) = bucket.iter().position(|(k, _)| k.borrow() == key) {
            let (_, v) = bucket.swap_remove(pos);
            self.len -= 1;
            Some(v)
        } else {
            None
        }
    }

    fn len(&self) -> usize {
        self.len
    }

    fn capacity(&self) -> usize {
        self.buckets.len()
    }

    fn clear(&mut self) {
        for bucket in &mut self.buckets {
            bucket.clear();
        }
        self.len = 0;
    }
}

// ---------------------------------------------------------------------------
// Iterator
// ---------------------------------------------------------------------------

pub struct ChainingIter<'a, K, V> {
    buckets: &'a [Vec<(K, V)>],
    bucket_idx: usize,
    item_idx: usize,
}

impl<'a, K, V> Iterator for ChainingIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        while self.bucket_idx < self.buckets.len() {
            if self.item_idx < self.buckets[self.bucket_idx].len() {
                let item = &self.buckets[self.bucket_idx][self.item_idx];
                self.item_idx += 1;
                return Some((&item.0, &item.1));
            }
            self.bucket_idx += 1;
            self.item_idx = 0;
        }
        None
    }
}

impl<K, V, S> IntoIterator for ChainingHashMap<K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    type Item = (K, V);
    type IntoIter = std::vec::IntoIter<(K, V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.buckets.into_iter().flatten().collect::<Vec<_>>().into_iter()
    }
}

impl<K, V, S> std::fmt::Debug for ChainingHashMap<K, V, S>
where
    K: Eq + Hash + std::fmt::Debug,
    V: std::fmt::Debug,
    S: BuildHasher,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChainingHashMap")
            .field("len", &self.len)
            .field("capacity", &self.buckets.len())
            .finish()
    }
}
