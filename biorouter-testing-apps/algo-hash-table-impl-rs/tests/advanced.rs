//! Advanced and edge-case tests.

use algo_hash_table_impl_rs::chaining::ChainingHashMap;
use algo_hash_table_impl_rs::common::{
    CollisionHasherBuilder, HashMap as HashMapTrait, ModHasherBuilder,
};
use algo_hash_table_impl_rs::linear::LinearProbingHashMap;
use algo_hash_table_impl_rs::robinhood::RobinHoodHashMap;

// ---------------------------------------------------------------------------
// Cluster analysis integration tests
// ---------------------------------------------------------------------------

mod cluster_analysis_tests {
    use algo_hash_table_impl_rs::cluster_analysis;

    #[test]
    fn analyze_all_reports_consistent_entry_counts() {
        let reports = cluster_analysis::analyze_all(100, 16);
        assert_eq!(reports.len(), 3);
        for r in &reports {
            assert_eq!(r.num_entries, 100);
            assert!(r.capacity > 0);
            assert!(r.load_factor > 0.0);
        }
    }

    #[test]
    fn analyze_total_collision_happens() {
        let reports = cluster_analysis::analyze_total_collision(30);
        assert_eq!(reports.len(), 2);
        // Both should report cluster_count of 1 (all in one bucket).
        for r in &reports {
            assert_eq!(r.cluster_count, 1);
            assert_eq!(r.num_entries, 30);
        }
    }

    #[test]
    fn robin_hood_probe_distance_report_present() {
        let reports = cluster_analysis::analyze_all(80, 16);
        let rh = reports.iter().find(|r| r.strategy.contains("Robin Hood")).unwrap();
        assert!(rh.avg_probe_distance.is_some());
        assert!(rh.max_probe_distance.is_some());
        let max = rh.max_probe_distance.unwrap();
        assert!(max < 80, "Robin Hood max probe distance {} too high", max);
    }

    #[test]
    fn linear_probing_tombstone_ratio_report_present() {
        let reports = cluster_analysis::analyze_all(80, 16);
        let lp = reports.iter().find(|r| r.strategy.contains("Linear")).unwrap();
        // No removals, so tombstone ratio should be 0.
        assert_eq!(lp.tombstone_ratio, Some(0.0));
    }
}

// ---------------------------------------------------------------------------
// Edge-case: insert into nearly-full table, resize correctness
// ---------------------------------------------------------------------------

mod edge_cases {
    use super::*;

    #[test]
    fn chaining_single_bucket_capacity() {
        let mut m = ChainingHashMap::<i32, i32>::with_capacity(1);
        assert_eq!(m.capacity(), 1);
        m.insert(1, 1);
        m.insert(2, 2);
        // Capacity should have grown.
        assert!(m.capacity() > 1);
        assert_eq!(m.get(&1), Some(&1));
        assert_eq!(m.get(&2), Some(&2));
    }

    #[test]
    fn linear_probing_single_slot_capacity() {
        let mut m = LinearProbingHashMap::<i32, i32>::with_capacity(1);
        assert_eq!(m.capacity(), 1);
        m.insert(1, 1);
        m.insert(2, 2);
        assert!(m.capacity() > 1);
        assert_eq!(m.get(&1), Some(&1));
        assert_eq!(m.get(&2), Some(&2));
    }

    #[test]
    fn robin_hood_single_slot_capacity() {
        let mut m = RobinHoodHashMap::<i32, i32>::with_capacity(1);
        assert_eq!(m.capacity(), 1);
        m.insert(1, 1);
        m.insert(2, 2);
        assert!(m.capacity() > 1);
        assert_eq!(m.get(&1), Some(&1));
        assert_eq!(m.get(&2), Some(&2));
    }

    #[test]
    fn chaining_high_load_factor() {
        let mut m = ChainingHashMap::<i32, i32>::with_capacity_and_load_factor(4, 0.99);
        for i in 0..100i32 {
            m.insert(i, i);
        }
        assert_eq!(m.len(), 100);
        for i in 0..100i32 {
            assert_eq!(m.get(&i), Some(&i));
        }
    }

    #[test]
    fn linear_high_load_factor() {
        let mut m = LinearProbingHashMap::<i32, i32>::with_capacity_and_load_factor(4, 0.9);
        for i in 0..100i32 {
            m.insert(i, i);
        }
        assert_eq!(m.len(), 100);
        for i in 0..100i32 {
            assert_eq!(m.get(&i), Some(&i));
        }
    }

    #[test]
    fn robin_hood_high_load_factor() {
        let mut m = RobinHoodHashMap::<i32, i32>::with_capacity_and_load_factor(4, 0.9);
        for i in 0..100i32 {
            m.insert(i, i);
        }
        assert_eq!(m.len(), 100);
        for i in 0..100i32 {
            assert_eq!(m.get(&i), Some(&i));
        }
    }

    #[test]
    fn insert_remove_reinsert_cycle() {
        let mut m = RobinHoodHashMap::<i32, i32>::new();
        for cycle in 0..5 {
            for i in 0..50i32 {
                m.insert(i, i + cycle * 100);
            }
            for i in 0..50i32 {
                assert_eq!(m.get(&i), Some(&(i + cycle * 100)));
            }
            for i in 0..50i32 {
                m.remove(&i);
            }
            assert_eq!(m.len(), 0);
        }
    }

    #[test]
    fn chaining_iter_after_remove() {
        let mut m = ChainingHashMap::<i32, i32>::new();
        for i in 0..10i32 {
            m.insert(i, i);
        }
        m.remove(&5);
        let mut keys: Vec<_> = m.keys().copied().collect();
        keys.sort();
        assert_eq!(keys, vec![0, 1, 2, 3, 4, 6, 7, 8, 9]);
    }

    #[test]
    fn linear_iter_after_remove() {
        let mut m = LinearProbingHashMap::<i32, i32>::new();
        for i in 0..10i32 {
            m.insert(i, i);
        }
        m.remove(&5);
        let mut keys: Vec<_> = m.keys().copied().collect();
        keys.sort();
        assert_eq!(keys, vec![0, 1, 2, 3, 4, 6, 7, 8, 9]);
    }

    #[test]
    fn robin_hood_iter_after_remove() {
        let mut m = RobinHoodHashMap::<i32, i32>::new();
        for i in 0..10i32 {
            m.insert(i, i);
        }
        m.remove(&5);
        let mut keys: Vec<_> = m.keys().copied().collect();
        keys.sort();
        assert_eq!(keys, vec![0, 1, 2, 3, 4, 6, 7, 8, 9]);
    }
}

// ---------------------------------------------------------------------------
// Collision-heavy hasher: stress tests
// ---------------------------------------------------------------------------

mod collision_stress {
    use super::*;

    #[test]
    fn linear_probing_total_collision_correctness() {
        let mut m = LinearProbingHashMap::<i32, i32, CollisionHasherBuilder>::with_capacity_and_load_factor(
            16, 0.99,
        );
        for i in 0..30i32 {
            m.insert(i, i * 100);
        }
        assert_eq!(m.len(), 30);
        for i in 0..30i32 {
            assert_eq!(m.get(&i), Some(&(i * 100)));
        }
        // Remove and verify.
        for i in 0..15i32 {
            m.remove(&i);
        }
        assert_eq!(m.len(), 15);
        for i in 0..15i32 {
            assert_eq!(m.get(&i), None);
        }
        for i in 15..30i32 {
            assert_eq!(m.get(&i), Some(&(i * 100)));
        }
    }

    #[test]
    fn robin_hood_total_collision_correctness() {
        let mut m = RobinHoodHashMap::<i32, i32, CollisionHasherBuilder>::with_capacity_and_load_factor(
            16, 0.99,
        );
        for i in 0..30i32 {
            m.insert(i, i * 100);
        }
        assert_eq!(m.len(), 30);
        for i in 0..30i32 {
            assert_eq!(m.get(&i), Some(&(i * 100)));
        }
        for i in 0..15i32 {
            m.remove(&i);
        }
        assert_eq!(m.len(), 15);
        for i in 0..15i32 {
            assert_eq!(m.get(&i), None);
        }
        for i in 15..30i32 {
            assert_eq!(m.get(&i), Some(&(i * 100)));
        }
    }

    #[test]
    fn robin_hood_total_collision_probe_distance() {
        let mut m = RobinHoodHashMap::<i32, i32, CollisionHasherBuilder>::with_capacity_and_load_factor(
            16, 0.99,
        );
        for i in 0..20i32 {
            m.insert(i, i);
        }
        // With total collision, all entries hash to slot 0.
        // Robin Hood distributes them: dist 0, 1, 2, ...
        // Max dist should be exactly 19 (entries 0..19).
        assert_eq!(m.max_probe_distance(), 19);
        // Avg dist should be (0 + 1 + ... + 19) / 20 = 9.5.
        let avg = m.avg_probe_distance();
        assert!((avg - 9.5).abs() < 0.01, "avg probe dist {} != 9.5", avg);
    }

    #[test]
    fn mod_hasher_8_correctness_all_impls() {
        let mut c = ChainingHashMap::<i32, i32, ModHasherBuilder>::with_capacity_and_load_factor(8, 0.9);
        let mut l = LinearProbingHashMap::<i32, i32, ModHasherBuilder>::with_capacity_and_load_factor(8, 0.9);
        let mut r = RobinHoodHashMap::<i32, i32, ModHasherBuilder>::with_capacity_and_load_factor(8, 0.9);

        for i in 0..50i32 {
            c.insert(i, i * 3);
            l.insert(i, i * 3);
            r.insert(i, i * 3);
        }

        for i in 0..50i32 {
            assert_eq!(c.get(&i), Some(&(i * 3)));
            assert_eq!(l.get(&i), Some(&(i * 3)));
            assert_eq!(r.get(&i), Some(&(i * 3)));
        }

        for i in (0..50i32).step_by(2) {
            c.remove(&i);
            l.remove(&i);
            r.remove(&i);
        }

        for i in 0..50i32 {
            let expected = if i % 2 == 0 { None } else { Some(&(i * 3)) };
            assert_eq!(c.get(&i), expected, "chaining key {}", i);
            assert_eq!(l.get(&i), expected, "linear key {}", i);
            assert_eq!(r.get(&i), expected, "robinhood key {}", i);
        }
    }
}

// ---------------------------------------------------------------------------
// Display / Debug formatting
// ---------------------------------------------------------------------------

mod fmt_tests {
    use super::*;

    #[test]
    fn chaining_debug_format() {
        let mut m = ChainingHashMap::<i32, i32>::new();
        m.insert(1, 1);
        let debug = format!("{:?}", m);
        assert!(debug.contains("ChainingHashMap"));
        assert!(debug.contains("len"));
    }

    #[test]
    fn linear_debug_format() {
        let mut m = LinearProbingHashMap::<i32, i32>::new();
        m.insert(1, 1);
        let debug = format!("{:?}", m);
        assert!(debug.contains("LinearProbingHashMap"));
        assert!(debug.contains("tombstones"));
    }

    #[test]
    fn robin_hood_debug_format() {
        let mut m = RobinHoodHashMap::<i32, i32>::new();
        m.insert(1, 1);
        let debug = format!("{:?}", m);
        assert!(debug.contains("RobinHoodHashMap"));
        assert!(debug.contains("len"));
    }
}
