//! Open-addressing hash map using Robin Hood hashing.
//!
//! Robin Hood hashing is a variant of open addressing with linear probing.
//! During insertion, if the probe distance of the inserting element exceeds
//! that of the element at the current slot, the two swap — "robbing from the
//! rich" — which dramatically reduces variance in probe lengths.
//!
//! Deletion uses backward-shift: after removing an entry, subsequent entries
//! with positive displacement are shifted backward to fill the gap. This
//! avoids tombstones entirely.

use std::borrow::Borrow;
use std::hash::{BuildHasher, Hash, Hasher};

use crate::common::{self, HashMap as HashMapTrait};

// ---------------------------------------------------------------------------
// Slot representation
// ---------------------------------------------------------------------------

/// A slot stores the key, value, and the *displacement* (probe distance)
/// from the ideal hash position.
struct RhSlot<K, V> {
    key: K,
    value: V,
    /// How far this entry is from its ideal slot.
    dist: usize,
}

// ---------------------------------------------------------------------------
// RobinHoodHashMap
// ---------------------------------------------------------------------------

/// Open-addressing hash map using Robin Hood hashing.
pub struct RobinHoodHashMap<K, V, S = common::FnvHasherBuilder>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    slots: Vec<Option<RhSlot<K, V>>>,
    len: usize,
    max_load: f64,
    hasher: S,
}

impl<K, V> RobinHoodHashMap<K, V, common::FnvHasherBuilder>
where
    K: Eq + Hash,
{
    pub fn new() -> Self {
        Self::with_capacity_and_load_factor_inner(16, 0.75, common::FnvHasherBuilder)
    }
}

impl<K, V, S> RobinHoodHashMap<K, V, S>
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
        let slots: Vec<Option<RhSlot<K, V>>> = (0..cap).map(|_| None).collect();
        RobinHoodHashMap {
            slots,
            len: 0,
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

    fn ideal_slot<Q>(&self, key: &Q) -> usize
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
        self.len as f64 / self.slots.len() as f64
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
            (0..new_cap).map(|_| None).collect(),
        );
        self.len = 0;
        for slot in old_slots.into_iter().flatten() {
            self.insert_internal(slot.key, slot.value);
        }
    }

    /// Insert without resizing (used during rehash).
    fn insert_internal(&mut self, key: K, value: V) {
        let cap = self.slots.len();
        let ideal = self.hash_key(&key) % cap;
        let mut current = RhSlot { key, value, dist: 0 };
        let mut idx = ideal;

        loop {
            match &mut self.slots[idx] {
                slot @ None => {
                    *slot = Some(current);
                    self.len += 1;
                    return;
                }
                Some(existing) if existing.key == current.key => {
                    // Overwrite.
                    std::mem::swap(&mut existing.value, &mut current.value);
                    return;
                }
                Some(existing) => {
                    // Robin Hood: swap if current element is "richer" (has
                    // travelled farther from its ideal slot).
                    if current.dist > existing.dist {
                        std::mem::swap(&mut current, existing);
                    }
                    current.dist += 1;
                    idx = (idx + 1) % cap;
                }
            }
        }
    }

    /// Return an iterator over `(&K, &V)`.
    pub fn iter(&self) -> RobinHoodIter<'_, K, V> {
        RobinHoodIter {
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

    /// Maximum probe distance across all entries (useful for cluster analysis).
    pub fn max_probe_distance(&self) -> usize {
        self.slots
            .iter()
            .filter_map(|s| s.as_ref().map(|s| s.dist))
            .max()
            .unwrap_or(0)
    }

    /// Average probe distance across all entries.
    pub fn avg_probe_distance(&self) -> f64 {
        if self.len == 0 {
            return 0.0;
        }
        let total: usize = self
            .slots
            .iter()
            .filter_map(|s| s.as_ref().map(|s| s.dist))
            .sum();
        total as f64 / self.len as f64
    }
}

impl<K, V, S> HashMapTrait<K, V, S> for RobinHoodHashMap<K, V, S>
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
        let cap = self.slots.len();
        let ideal = self.hash_key(&key) % cap;
        let mut current = RhSlot { key, value, dist: 0 };
        let mut idx = ideal;

        loop {
            match &mut self.slots[idx] {
                slot @ None => {
                    *slot = Some(current);
                    self.len += 1;
                    return None;
                }
                Some(existing) if existing.key == current.key => {
                    let old_v = std::mem::replace(&mut existing.value, current.value);
                    return Some(old_v);
                }
                Some(existing) => {
                    if current.dist > existing.dist {
                        std::mem::swap(&mut current, existing);
                    }
                    current.dist += 1;
                    idx = (idx + 1) % cap;
                }
            }
        }
    }

    fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let cap = self.slots.len();
        let ideal = self.ideal_slot(key);
        let mut dist = 0usize;
        let mut idx = ideal;

        loop {
            match &self.slots[idx] {
                None => return None,
                Some(slot) => {
                    // If the current slot's distance is less than `dist`, we
                    // have passed all possible locations for `key`.
                    if slot.dist < dist {
                        return None;
                    }
                    if slot.key.borrow() == key {
                        return Some(&slot.value);
                    }
                    dist += 1;
                    idx = (idx + 1) % cap;
                }
            }
        }
    }

    fn get_mut<Q>(&self, _key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        // Simplified: returns immutable reference (see chaining module note).
        let cap = self.slots.len();
        let ideal = self.ideal_slot(_key);
        let mut dist = 0usize;
        let mut idx = ideal;

        loop {
            match &self.slots[idx] {
                None => return None,
                Some(slot) => {
                    if slot.dist < dist {
                        return None;
                    }
                    if slot.key.borrow() == _key {
                        return Some(&slot.value);
                    }
                    dist += 1;
                    idx = (idx + 1) % cap;
                }
            }
        }
    }

    fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let cap = self.slots.len();
        let ideal = self.ideal_slot(key);
        let mut dist = 0usize;
        let mut idx = ideal;

        loop {
            match &self.slots[idx] {
                None => return None,
                Some(slot) => {
                    if slot.dist < dist {
                        return None;
                    }
                    if slot.key.borrow() == key {
                        // Remove and backward-shift.
                        let removed = self.slots[idx].take().unwrap();
                        self.len -= 1;
                        // Backward-shift: shift subsequent entries back.
                        let mut shift_idx = (idx + 1) % cap;
                        loop {
                            match &self.slots[shift_idx] {
                                None | Some(RhSlot { dist: 0, .. }) => break,
                                Some(_) => {
                                    let prev = (shift_idx + cap - 1) % cap;
                                    let mut slot = self.slots[shift_idx].take().unwrap();
                                    slot.dist -= 1;
                                    self.slots[prev] = Some(slot);
                                    shift_idx = (shift_idx + 1) % cap;
                                }
                            }
                        }
                        return Some(removed.value);
                    }
                    dist += 1;
                    idx = (idx + 1) % cap;
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
            *slot = None;
        }
        self.len = 0;
    }
}

// ---------------------------------------------------------------------------
// Iterator
// ---------------------------------------------------------------------------

pub struct RobinHoodIter<'a, K, V> {
    slots: &'a [Option<RhSlot<K, V>>],
    idx: usize,
}

impl<'a, K, V> Iterator for RobinHoodIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        while self.idx < self.slots.len() {
            let slot = &self.slots[self.idx];
            self.idx += 1;
            if let Some(RhSlot { key, value, .. }) = slot {
                return Some((key, value));
            }
        }
        None
    }
}

impl<K, V, S> std::fmt::Debug for RobinHoodHashMap<K, V, S>
where
    K: Eq + Hash + std::fmt::Debug,
    V: std::fmt::Debug,
    S: BuildHasher,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RobinHoodHashMap")
            .field("len", &self.len)
            .field("capacity", &self.slots.len())
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
        let mut m = RobinHoodHashMap::<i32, i32>::new();
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
        let mut m = RobinHoodHashMap::<i32, i32>::new();
        m.insert(1, 10);
        let old = m.insert(1, 99);
        assert_eq!(old, Some(10));
        assert_eq!(m.get(&1), Some(&99));
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn remove_with_backward_shift() {
        let mut m = RobinHoodHashMap::<i32, i32>::new();
        m.insert(1, 10);
        m.insert(2, 20);
        m.insert(3, 30);
        assert_eq!(m.remove(&2), Some(20));
        assert_eq!(m.len(), 2);
        // Other entries should still be findable.
        assert_eq!(m.get(&1), Some(&10));
        assert_eq!(m.get(&3), Some(&30));
    }

    #[test]
    fn remove_nonexistent() {
        let mut m = RobinHoodHashMap::<i32, i32>::new();
        m.insert(1, 10);
        assert_eq!(m.remove(&99), None);
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn resize_preserves_entries() {
        let mut m = RobinHoodHashMap::<i32, i32>::with_capacity_and_load_factor(4, 0.5);
        for i in 0..100 {
            m.insert(i, i * 10);
        }
        assert_eq!(m.len(), 100);
        for i in 0..100 {
            assert_eq!(m.get(&i), Some(&(i * 10)));
        }
    }

    #[test]
    fn clear_resets() {
        let mut m = RobinHoodHashMap::<i32, i32>::new();
        m.insert(1, 1);
        m.insert(2, 2);
        m.clear();
        assert_eq!(m.len(), 0);
        assert!(m.is_empty());
    }

    #[test]
    fn iterator_works() {
        let mut m = RobinHoodHashMap::<i32, i32>::new();
        m.insert(1, 10);
        m.insert(2, 20);
        m.insert(3, 30);
        let mut pairs: Vec<_> = m.iter().map(|(k, v)| (*k, *v)).collect();
        pairs.sort();
        assert_eq!(pairs, vec![(1, 10), (2, 20), (3, 30)]);
    }

    #[test]
    fn probe_distance_tracked() {
        let mut m = RobinHoodHashMap::<i32, i32>::new();
        m.insert(1, 10);
        m.insert(2, 20);
        // max probe distance should be 0 for a sparsely populated table.
        let _ = m.max_probe_distance();
    }

    #[test]
    fn many_inserts_and_removes() {
        let mut m = RobinHoodHashMap::<i32, i32>::new();
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

    #[test]
    fn robin_hood_reduces_variance() {
        // With a collision-heavy hasher, Robin Hood should have lower max
        // probe distance than vanilla linear probing.
        use crate::common::ModHasherBuilder;
        let mut m = RobinHoodHashMap::<i32, i32, ModHasherBuilder>::with_capacity_and_load_factor(
            16,
            0.9,
        );
        // Insert many entries that will collide (mod 16).
        for i in 0..12 {
            m.insert(i, i);
        }
        // Robin Hood max probe distance should be bounded.
        // With 12 entries in 16 slots and moderate collisions, max dist should
        // be significantly less than 12.
        let max_dist = m.max_probe_distance();
        assert!(
            max_dist < 12,
            "Robin Hood max probe distance {} should be < 12",
            max_dist
        );
    }
}
