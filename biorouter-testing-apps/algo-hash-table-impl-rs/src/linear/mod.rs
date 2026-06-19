//! Open-addressing hash map with linear probing.
//!
//! Uses tombstone markers for deletion. Resizes (doubles) when the load
//! factor exceeds the configured maximum. Tombstones are reclaimed on
//! resize and can optionally be cleaned up during insert.

use std::borrow::Borrow;
use std::hash::{BuildHasher, Hash, Hasher};

use crate::common::{self, HashMap as HashMapTrait};

// ---------------------------------------------------------------------------
// Slot representation
// ---------------------------------------------------------------------------

enum Slot<K, V> {
    Empty,
    Occupied { key: K, value: V },
    Tombstone,
}

impl<K, V> Slot<K, V> {
    #[allow(dead_code)]
    fn is_empty(&self) -> bool {
        matches!(self, Slot::Empty)
    }

    #[allow(dead_code)]
    fn is_occupied(&self) -> bool {
        matches!(self, Slot::Occupied { .. })
    }

    #[allow(dead_code)]
    fn is_tombstone(&self) -> bool {
        matches!(self, Slot::Tombstone)
    }
}

// ---------------------------------------------------------------------------
// LinearProbingHashMap
// ---------------------------------------------------------------------------

/// Open-addressing hash map with linear probing and tombstone deletion.
pub struct LinearProbingHashMap<K, V, S = common::FnvHasherBuilder>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    slots: Vec<Slot<K, V>>,
    len: usize,
    tombstones: usize,
    max_load: f64,
    hasher: S,
}

impl<K, V> LinearProbingHashMap<K, V, common::FnvHasherBuilder>
where
    K: Eq + Hash,
{
    pub fn new() -> Self {
        Self::with_capacity_and_load_factor_inner(16, 0.75, common::FnvHasherBuilder)
    }
}

impl<K, V, S> LinearProbingHashMap<K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher + Default,
{
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_load_factor_inner(capacity, 0.75, S::default())
    }

    pub fn with_capacity_and_load_factor(capacity: usize, max_load: f64) -> Self {
        Self::with_capacity_and_load_factor_inner(capacity, max_load, S::default())
    }

    fn with_capacity_and_load_factor_inner(capacity: usize, max_load: f64, hasher: S) -> Self {
        let cap = common::next_power_of_two(capacity.max(1));
        let slots = (0..cap).map(|_| Slot::Empty).collect();
        LinearProbingHashMap {
            slots,
            len: 0,
            tombstones: 0,
            max_load: max_load.clamp(0.1, 1.0),
            hasher,
        }
    }

    fn hash_key<Q>(&self, key: &Q) -> usize
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mut hasher = self.hasher.build_hasher();
        key.hash(&mut hasher);
        hasher.finish() as usize
    }

    #[allow(dead_code)]
    fn probe<Q>(&self, key: &Q) -> usize
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.hash_key(key) % self.slots.len()
    }

    fn load_factor_internal(&self) -> f64 {
        if self.slots.is_empty() {
            return 0.0;
        }
        (self.len + self.tombstones) as f64 / self.slots.len() as f64
    }

    fn maybe_resize(&mut self) {
        if self.slots.is_empty() || self.load_factor_internal() <= self.max_load {
            return;
        }
        self.resize();
    }

    fn resize(&mut self) {
        let new_cap = self.slots.len() * 2;
        let old_slots = std::mem::replace(
            &mut self.slots,
            (0..new_cap).map(|_| Slot::Empty).collect(),
        );
        self.len = 0;
        self.tombstones = 0;
        for slot in old_slots {
            if let Slot::Occupied { key, value } = slot {
                self.insert_internal(key, value);
            }
        }
    }

    /// Insert without resizing (used during rehash).
    fn insert_internal(&mut self, key: K, value: V) {
        let mut idx = self.hash_key(&key) % self.slots.len();
        loop {
            match &self.slots[idx] {
                Slot::Empty | Slot::Tombstone => {
                    self.slots[idx] = Slot::Occupied { key, value };
                    self.len += 1;
                    return;
                }
                Slot::Occupied { key: existing, .. } if *existing == key => {
                    self.slots[idx] = Slot::Occupied { key, value };
                    return;
                }
                _ => {
                    idx = (idx + 1) % self.slots.len();
                }
            }
        }
    }

    /// Return an iterator over `(&K, &V)`.
    pub fn iter(&self) -> LinearIter<'_, K, V> {
        LinearIter {
            slots: &self.slots,
            idx: 0,
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

    /// Number of tombstone slots (for analysis).
    pub fn tombstone_count(&self) -> usize {
        self.tombstones
    }
}

impl<K, V, S> HashMapTrait<K, V, S> for LinearProbingHashMap<K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher + Default,
{
    fn new() -> Self {
        Self::with_capacity_and_load_factor_inner(16, 0.75, S::default())
    }

    fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity(capacity)
    }

    fn with_capacity_and_load_factor(capacity: usize, max_load: f64) -> Self {
        Self::with_capacity_and_load_factor(capacity, max_load)
    }

    fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.maybe_resize();
        let mut idx = self.hash_key(&key) % self.slots.len();
        let mut first_tombstone: Option<usize> = None;

        loop {
            match &self.slots[idx] {
                Slot::Empty => {
                    // Insert at first tombstone if we saw one, otherwise here.
                    let insert_at = first_tombstone.unwrap_or(idx);
                    // If inserting at a tombstone, decrement tombstone count.
                    if first_tombstone.is_some() {
                        self.tombstones -= 1;
                    }
                    self.slots[insert_at] = Slot::Occupied { key, value };
                    self.len += 1;
                    return None;
                }
                Slot::Tombstone => {
                    if first_tombstone.is_none() {
                        first_tombstone = Some(idx);
                    }
                    idx = (idx + 1) % self.slots.len();
                }
                Slot::Occupied { key: existing, .. } if *existing == key => {
                    // Overwrite.
                    if first_tombstone.is_some() {
                        // Shift this occupied slot to the tombstone to improve
                        // future probe performance (optional optimisation).
                        let tomb = first_tombstone.unwrap();
                        self.tombstones -= 1;
                        let old = std::mem::replace(
                            &mut self.slots[idx],
                            Slot::Tombstone,
                        );
                        self.tombstones += 1;
                        self.slots[tomb] = Slot::Occupied { key, value };
                        if let Slot::Occupied { value: old_v, .. } = old {
                            return Some(old_v);
                        }
                        unreachable!()
                    } else {
                        let old = std::mem::replace(
                            &mut self.slots[idx],
                            Slot::Occupied { key, value },
                        );
                        if let Slot::Occupied { value: old_v, .. } = old {
                            return Some(old_v);
                        }
                        unreachable!()
                    }
                }
                Slot::Occupied { .. } => {
                    idx = (idx + 1) % self.slots.len();
                }
            }
        }
    }

    fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mut idx = self.hash_key(key) % self.slots.len();
        loop {
            match &self.slots[idx] {
                Slot::Empty => return None,
                Slot::Occupied { key: k, value } if k.borrow() == key => {
                    return Some(value);
                }
                _ => {
                    idx = (idx + 1) % self.slots.len();
                }
            }
        }
    }

    fn get_mut<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        // Simplified: returns immutable reference.
        let mut idx = self.hash_key(key) % self.slots.len();
        loop {
            match &self.slots[idx] {
                Slot::Empty => return None,
                Slot::Occupied { key: k, value } if k.borrow() == key => {
                    return Some(value);
                }
                _ => {
                    idx = (idx + 1) % self.slots.len();
                }
            }
        }
    }

    fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mut idx = self.hash_key(key) % self.slots.len();
        loop {
            match &self.slots[idx] {
                Slot::Empty => return None,
                Slot::Occupied { key: k, .. } if k.borrow() == key => {
                    let old = std::mem::replace(&mut self.slots[idx], Slot::Tombstone);
                    self.len -= 1;
                    self.tombstones += 1;
                    if let Slot::Occupied { value, .. } = old {
                        return Some(value);
                    }
                    unreachable!()
                }
                _ => {
                    idx = (idx + 1) % self.slots.len();
                }
            }
        }
    }

    fn len(&self) -> usize {
        self.len
    }

    fn capacity(&self) -> usize {
        self.slots.len()
    }

    fn clear(&mut self) {
        for slot in &mut self.slots {
            *slot = Slot::Empty;
        }
        self.len = 0;
        self.tombstones = 0;
    }
}

// ---------------------------------------------------------------------------
// Iterator
// ---------------------------------------------------------------------------

pub struct LinearIter<'a, K, V> {
    slots: &'a [Slot<K, V>],
    idx: usize,
}

impl<'a, K, V> Iterator for LinearIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        while self.idx < self.slots.len() {
            let slot = &self.slots[self.idx];
            self.idx += 1;
            if let Slot::Occupied { key, value } = slot {
                return Some((key, value));
            }
        }
        None
    }
}

impl<K, V, S> std::fmt::Debug for LinearProbingHashMap<K, V, S>
where
    K: Eq + Hash + std::fmt::Debug,
    V: std::fmt::Debug,
    S: BuildHasher,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinearProbingHashMap")
            .field("len", &self.len)
            .field("capacity", &self.slots.len())
            .field("tombstones", &self.tombstones)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::HashMap as HashMapTrait;

    #[test]
    fn basic_insert_get() {
        let mut m = LinearProbingHashMap::<i32, i32>::new();
        assert!(m.is_empty());
        m.insert(1, 10);
        m.insert(2, 20);
        assert_eq!(m.len(), 2);
        assert_eq!(m.get(&1), Some(&10));
        assert_eq!(m.get(&2), Some(&20));
        assert_eq!(m.get(&3), None);
    }

    #[test]
    fn insert_overwrite() {
        let mut m = LinearProbingHashMap::<i32, i32>::new();
        m.insert(1, 10);
        let old = m.insert(1, 99);
        assert_eq!(old, Some(10));
        assert_eq!(m.get(&1), Some(&99));
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn remove_creates_tombstone() {
        let mut m = LinearProbingHashMap::<i32, i32>::new();
        m.insert(1, 10);
        m.insert(2, 20);
        assert_eq!(m.remove(&1), Some(10));
        assert_eq!(m.tombstones, 1);
        assert_eq!(m.len(), 1);
        // Key 2 should still be findable past the tombstone.
        assert_eq!(m.get(&2), Some(&20));
        assert_eq!(m.get(&1), None);
    }

    #[test]
    fn remove_nonexistent() {
        let mut m = LinearProbingHashMap::<i32, i32>::new();
        m.insert(1, 10);
        assert_eq!(m.remove(&99), None);
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn resize_preserves_entries() {
        let mut m = LinearProbingHashMap::<i32, i32>::with_capacity_and_load_factor(4, 0.5);
        for i in 0..100 {
            m.insert(i, i * 10);
        }
        assert_eq!(m.len(), 100);
        for i in 0..100 {
            assert_eq!(m.get(&i), Some(&(i * 10)));
        }
        // After resize, tombstones should be 0 (they are not carried over).
        assert_eq!(m.tombstones, 0);
    }

    #[test]
    fn tombstone_insertion_reuses_slot() {
        let mut m = LinearProbingHashMap::<i32, i32>::with_capacity(4);
        m.insert(0, 0);
        m.insert(1, 1);
        m.insert(2, 2);
        // Remove and re-insert should work.
        m.remove(&1);
        assert_eq!(m.len(), 2);
        m.insert(100, 100);
        assert_eq!(m.get(&100), Some(&100));
    }

    #[test]
    fn clear_resets() {
        let mut m = LinearProbingHashMap::<i32, i32>::new();
        m.insert(1, 1);
        m.insert(2, 2);
        m.clear();
        assert_eq!(m.len(), 0);
        assert_eq!(m.tombstones, 0);
        assert!(m.is_empty());
    }

    #[test]
    fn iterator_works() {
        let mut m = LinearProbingHashMap::<i32, i32>::new();
        m.insert(1, 10);
        m.insert(2, 20);
        m.insert(3, 30);
        let mut pairs: Vec<_> = m.iter().map(|(k, v)| (*k, *v)).collect();
        pairs.sort();
        assert_eq!(pairs, vec![(1, 10), (2, 20), (3, 30)]);
    }

    #[test]
    fn many_inserts_and_removes() {
        let mut m = LinearProbingHashMap::<i32, i32>::new();
        for i in 0..200 {
            m.insert(i, i);
        }
        assert_eq!(m.len(), 200);
        for i in 0..100 {
            assert_eq!(m.remove(&i), Some(i));
        }
        assert_eq!(m.len(), 100);
        for i in 100..200 {
            assert_eq!(m.get(&i), Some(&i));
        }
    }
}
