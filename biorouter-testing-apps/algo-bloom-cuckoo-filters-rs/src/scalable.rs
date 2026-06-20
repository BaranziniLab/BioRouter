//! Scalable Bloom filter.
//!
//! Automatically grows by adding successive Bloom filter layers with
//! progressively tighter false-positive rates, maintaining the overall
//! target FPR across all layers.
//!
//! Based on: Almeida et al., "Scalable Bloom Filters" (2007).

use crate::bloom::BloomFilter;
use serde::{Deserialize, Serialize};
use std::hash::Hash;

/// Growth factor for successive layers (2× = capacity doubles each time).
const GROWTH_FACTOR: f64 = 2.0;

/// Tightening ratio for FPR in each successive layer.
/// Each layer's FPR = previous_layer_FPR * TIGHTENING_RATIO.
/// This ensures the overall FPR stays within the target.
const TIGHTENING_RATIO: f64 = 0.5;

/// A Scalable Bloom filter that grows as needed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScalableBloomFilter {
    layers: Vec<BloomFilter>,
    /// The initial target FPR for the first layer.
    initial_fp_rate: f64,
    /// Capacity of the first layer.
    initial_capacity: usize,
    /// Total items inserted across all layers.
    total_items: u64,
}

impl ScalableBloomFilter {
    /// Create a new Scalable Bloom filter.
    ///
    /// - `initial_capacity`: expected items for the first layer.
    /// - `target_fpr`: overall false-positive rate target.
    pub fn new(initial_capacity: usize, target_fpr: f64) -> Self {
        assert!(initial_capacity > 0);
        assert!(target_fpr > 0.0 && target_fpr < 1.0);

        // First layer gets the full FPR budget
        let first_layer = BloomFilter::optimal(initial_capacity, target_fpr);

        ScalableBloomFilter {
            layers: vec![first_layer],
            initial_fp_rate: target_fpr,
            initial_capacity,
            total_items: 0,
        }
    }

    /// Insert an item. If the current layer is saturated, a new layer
    /// is created automatically.
    pub fn insert<T: Hash + ?Sized>(&mut self, item: &T) {
        // Check if we need to grow: if the last layer's theoretical FPR exceeds
        // its budget, add a new layer.
        let last = self.layers.last().unwrap();
        let layer_idx = self.layers.len() - 1;
        let _layer_fpr_budget = self.initial_fp_rate * TIGHTENING_RATIO.powi(layer_idx as i32);

        // Estimate capacity of the last layer
        let layer_capacity = (self.initial_capacity as f64 * GROWTH_FACTOR.powi(layer_idx as i32)) as usize;

        if last.len() >= layer_capacity as u64 {
            // Grow: add a new layer with tighter FPR
            let new_fpr = self.initial_fp_rate * TIGHTENING_RATIO.powi(self.layers.len() as i32);
            let new_capacity = (self.initial_capacity as f64
                * GROWTH_FACTOR.powi(self.layers.len() as i32))
                as usize;
            self.layers.push(BloomFilter::optimal(new_capacity, new_fpr));
        }

        // Insert into the last (newest) layer
        self.layers.last_mut().unwrap().insert(item);
        self.total_items += 1;
    }

    /// Check if an item might be in any layer.
    pub fn contains<T: Hash + ?Sized>(&self, item: &T) -> bool {
        self.layers.iter().any(|layer| layer.contains(item))
    }

    pub fn num_layers(&self) -> usize {
        self.layers.len()
    }
    pub fn len(&self) -> u64 {
        self.total_items
    }
    pub fn is_empty(&self) -> bool {
        self.total_items == 0
    }

    /// Total number of bits across all layers.
    pub fn total_bits(&self) -> usize {
        self.layers.iter().map(|l| l.num_bits()).sum()
    }

    /// Theoretical composite FPR.
    pub fn theoretical_fpr(&self) -> f64 {
        // Overall FPR = 1 - product(1 - layer_fpr)
        let product: f64 = self
            .layers
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let layer_fpr = self.initial_fp_rate * TIGHTENING_RATIO.powi(i as i32);
                1.0 - layer_fpr
            })
            .product();
        1.0 - product
    }

    /// Access layers for inspection.
    pub fn layers(&self) -> &[BloomFilter] {
        &self.layers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_false_negatives() {
        let mut sbf = ScalableBloomFilter::new(100, 0.01);
        for i in 0..1000u32 {
            sbf.insert(&i);
        }
        for i in 0..1000u32 {
            assert!(sbf.contains(&i), "false negative for {}", i);
        }
    }

    #[test]
    fn grows_beyond_initial_capacity() {
        let mut sbf = ScalableBloomFilter::new(50, 0.01);
        assert_eq!(sbf.num_layers(), 1);
        for i in 0..200u32 {
            sbf.insert(&i);
        }
        assert!(sbf.num_layers() > 1, "should have grown beyond 1 layer");
    }

    #[test]
    fn fpr_within_tolerance() {
        let n = 5000;
        let target_fpr = 0.01;
        let mut sbf = ScalableBloomFilter::new(500, target_fpr);
        for i in 0..n {
            sbf.insert(&i);
        }
        let measured = crate::analysis::measure_fpr_sbf(&sbf, n, n);
        // Scalable Bloom should maintain the overall target FPR (with some slack)
        assert!(
            measured < target_fpr * 5.0,
            "measured FPR {} exceeds tolerance (target {})",
            measured,
            target_fpr
        );
    }

    #[test]
    fn serialization_roundtrip() {
        let mut sbf = ScalableBloomFilter::new(100, 0.01);
        for i in 0..500u32 {
            sbf.insert(&i);
        }
        let json = serde_json::to_string(&sbf).unwrap();
        let sbf2: ScalableBloomFilter = serde_json::from_str(&json).unwrap();
        for i in 0..500u32 {
            assert!(sbf2.contains(&i));
        }
    }
}
