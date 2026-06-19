//! Cluster analysis for hash table implementations.
//!
//! Provides utilities to measure and compare collision behaviour:
//! cluster lengths, probe distances, tombstone ratios, etc.

use crate::chaining::ChainingHashMap;
use crate::common::{HashMap as HashMapTrait, CollisionHasherBuilder, ModHasherBuilder};
use crate::linear::LinearProbingHashMap;
use crate::robinhood::RobinHoodHashMap;

// ---------------------------------------------------------------------------
// Analysis result
// ---------------------------------------------------------------------------

/// Cluster analysis results for a single hash table.
#[derive(Debug, Clone)]
pub struct ClusterReport {
    pub strategy: String,
    pub num_entries: usize,
    pub capacity: usize,
    pub load_factor: f64,
    /// Number of contiguous occupied runs in the internal array.
    pub cluster_count: usize,
    /// Length of the longest contiguous occupied run.
    pub max_cluster_length: usize,
    /// Average cluster length.
    pub avg_cluster_length: f64,
    /// Tombstone ratio (only meaningful for open-addressing).
    pub tombstone_ratio: Option<f64>,
    /// Average probe distance (Robin Hood only).
    pub avg_probe_distance: Option<f64>,
    /// Maximum probe distance (Robin Hood only).
    pub max_probe_distance: Option<usize>,
}

impl std::fmt::Display for ClusterReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== {} ===", self.strategy)?;
        writeln!(f, "  Entries:          {}", self.num_entries)?;
        writeln!(f, "  Capacity:         {}", self.capacity)?;
        writeln!(f, "  Load factor:      {:.4}", self.load_factor)?;
        writeln!(f, "  Cluster count:    {}", self.cluster_count)?;
        writeln!(f, "  Max cluster len:  {}", self.max_cluster_length)?;
        writeln!(f, "  Avg cluster len:  {:.2}", self.avg_cluster_length)?;
        if let Some(tr) = self.tombstone_ratio {
            writeln!(f, "  Tombstone ratio:  {:.4}", tr)?;
        }
        if let Some(apd) = self.avg_probe_distance {
            writeln!(f, "  Avg probe dist:   {:.2}", apd)?;
        }
        if let Some(mpd) = self.max_probe_distance {
            writeln!(f, "  Max probe dist:   {}", mpd)?;
        }
        Ok(())
    }
}

/// Run cluster analysis on all three strategies using a collision-heavy hasher.
///
/// Inserts `n` entries into each implementation (with a mod-hasher that
/// maps keys into `modulus` buckets) and reports cluster statistics.
pub fn analyze_all(n: usize, modulus: u64) -> Vec<ClusterReport> {
    let mut reports = Vec::new();

    // --- Chaining ---
    {
        let mut m = ChainingHashMap::<i32, i32, ModHasherBuilder>::with_capacity_and_load_factor(
            modulus as usize,
            0.95,
        );
        for i in 0..n as i32 {
            m.insert(i, i);
        }
        // For chaining, "clusters" are bucket chain lengths.
        // We can iterate internal state indirectly: report len vs capacity.
        reports.push(ClusterReport {
            strategy: format!("Chaining (mod {})", modulus),
            num_entries: m.len(),
            capacity: m.capacity(),
            load_factor: m.load_factor(),
            cluster_count: m.capacity(), // each bucket is a "cluster"
            max_cluster_length: 0, // not directly accessible
            avg_cluster_length: m.len() as f64 / m.capacity() as f64,
            tombstone_ratio: None,
            avg_probe_distance: None,
            max_probe_distance: None,
        });
    }

    // --- Linear Probing ---
    {
        let mut m = LinearProbingHashMap::<i32, i32, ModHasherBuilder>::with_capacity_and_load_factor(
            modulus as usize,
            0.95,
        );
        for i in 0..n as i32 {
            m.insert(i, i);
        }
        // We can't directly inspect internal slots, but we know the
        // tombstone count and can estimate clusters from iteration order.
        let tombstones = m.tombstone_count();
        reports.push(ClusterReport {
            strategy: format!("Linear Probing (mod {})", modulus),
            num_entries: m.len(),
            capacity: m.capacity(),
            load_factor: m.load_factor(),
            cluster_count: 0, // would need internal access
            max_cluster_length: 0,
            avg_cluster_length: 0.0,
            tombstone_ratio: Some(tombstones as f64 / m.capacity() as f64),
            avg_probe_distance: None,
            max_probe_distance: None,
        });
    }

    // --- Robin Hood ---
    {
        let mut m = RobinHoodHashMap::<i32, i32, ModHasherBuilder>::with_capacity_and_load_factor(
            modulus as usize,
            0.95,
        );
        for i in 0..n as i32 {
            m.insert(i, i);
        }
        let max_dist = m.max_probe_distance();
        let avg_dist = m.avg_probe_distance();
        reports.push(ClusterReport {
            strategy: format!("Robin Hood (mod {})", modulus),
            num_entries: m.len(),
            capacity: m.capacity(),
            load_factor: m.load_factor(),
            cluster_count: 0,
            max_cluster_length: max_dist,
            avg_cluster_length: avg_dist,
            tombstone_ratio: None,
            avg_probe_distance: Some(avg_dist),
            max_probe_distance: Some(max_dist),
        });
    }

    reports
}

/// Run cluster analysis using the worst-case collision hasher (all keys
/// hash to the same bucket).
pub fn analyze_total_collision(n: usize) -> Vec<ClusterReport> {
    let mut reports = Vec::new();

    // With total collision, chaining just makes one long chain.
    {
        let mut m = ChainingHashMap::<i32, i32, CollisionHasherBuilder>::with_capacity_and_load_factor(
            16,
            0.99,
        );
        for i in 0..n as i32 {
            m.insert(i, i);
        }
        reports.push(ClusterReport {
            strategy: "Chaining (total collision)".to_string(),
            num_entries: m.len(),
            capacity: m.capacity(),
            load_factor: m.load_factor(),
            cluster_count: 1,
            max_cluster_length: n,
            avg_cluster_length: n as f64,
            tombstone_ratio: None,
            avg_probe_distance: None,
            max_probe_distance: None,
        });
    }

    // Robin Hood with total collision.
    {
        let mut m = RobinHoodHashMap::<i32, i32, CollisionHasherBuilder>::with_capacity_and_load_factor(
            16,
            0.99,
        );
        for i in 0..n as i32 {
            m.insert(i, i);
        }
        reports.push(ClusterReport {
            strategy: "Robin Hood (total collision)".to_string(),
            num_entries: m.len(),
            capacity: m.capacity(),
            load_factor: m.load_factor(),
            cluster_count: 1,
            max_cluster_length: n,
            avg_cluster_length: n as f64,
            tombstone_ratio: None,
            avg_probe_distance: Some(m.avg_probe_distance()),
            max_probe_distance: Some(m.max_probe_distance()),
        });
    }

    reports
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_all_returns_three_reports() {
        let reports = analyze_all(50, 8);
        assert_eq!(reports.len(), 3);
        for r in &reports {
            assert_eq!(r.num_entries, 50);
        }
    }

    #[test]
    fn analyze_total_collision_reports() {
        let reports = analyze_total_collision(20);
        assert_eq!(reports.len(), 2);
    }

    #[test]
    fn robin_hood_has_bounded_probe_distance() {
        let reports = analyze_all(100, 16);
        let rh = reports.iter().find(|r| r.strategy.contains("Robin Hood")).unwrap();
        // Robin Hood max probe distance should be significantly less than
        // the number of entries.
        let max = rh.max_probe_distance.unwrap();
        assert!(max < 100, "Robin Hood max probe distance {} too high", max);
    }
}
