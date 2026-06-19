//! Integration tests comparing all hash table implementations.

use algo_hash_table_impl_rs::chaining::ChainingHashMap;
use algo_hash_table_impl_rs::common::{
    CollisionHasherBuilder, HashMap as HashMapTrait, ModHasherBuilder,
};
use algo_hash_table_impl_rs::linear::LinearProbingHashMap;
use algo_hash_table_impl_rs::robinhood::RobinHoodHashMap;

// ---------------------------------------------------------------------------
// Macro: generate the same test suite for each implementation
// ---------------------------------------------------------------------------

macro_rules! impl_tests {
    ($mod_name:ident, $map_ty:ty, $new:expr) => {
        mod $mod_name {
            use super::*;

            #[test]
            fn test_insert_and_get() {
                let mut m: $map_ty = $new;
                assert!(m.is_empty());
                assert_eq!(m.len(), 0);

                for i in 0..100i32 {
                    assert_eq!(m.insert(i, i * 10), None);
                }
                assert_eq!(m.len(), 100);

                for i in 0..100i32 {
                    assert_eq!(m.get(&i), Some(&(i * 10)));
                }
                assert_eq!(m.get(&1000), None);
            }

            #[test]
            fn test_overwrite() {
                let mut m: $map_ty = $new;
                m.insert(42i32, 1);
                assert_eq!(m.insert(42i32, 2), Some(1));
                assert_eq!(m.get(&42i32), Some(&2));
                assert_eq!(m.len(), 1);
            }

            #[test]
            fn test_remove() {
                let mut m: $map_ty = $new;
                for i in 0..50i32 {
                    m.insert(i, i);
                }
                for i in (0..50i32).step_by(2) {
                    assert_eq!(m.remove(&i), Some(i));
                }
                assert_eq!(m.len(), 25);

                for i in 0..50i32 {
                    if i % 2 == 0 {
                        assert_eq!(m.get(&i), None);
                    } else {
                        assert_eq!(m.get(&i), Some(&i));
                    }
                }
            }

            #[test]
            fn test_remove_nonexistent() {
                let mut m: $map_ty = $new;
                m.insert(1i32, 1);
                assert_eq!(m.remove(&999i32), None);
                assert_eq!(m.len(), 1);
            }

            #[test]
            fn test_clear() {
                let mut m: $map_ty = $new;
                for i in 0..100i32 {
                    m.insert(i, i);
                }
                m.clear();
                assert_eq!(m.len(), 0);
                assert!(m.is_empty());
                m.insert(0i32, 0);
                assert_eq!(m.get(&0i32), Some(&0));
            }

            #[test]
            fn test_contains_key() {
                let mut m: $map_ty = $new;
                m.insert(5i32, 5);
                assert!(m.contains_key(&5i32));
                assert!(!m.contains_key(&6i32));
            }

            #[test]
            fn test_iterator() {
                let mut m: $map_ty = $new;
                for i in 0..20i32 {
                    m.insert(i, i * 2);
                }
                let mut pairs: Vec<_> = m.iter().map(|(k, v)| (*k, *v)).collect();
                pairs.sort();
                let expected: Vec<_> = (0..20i32).map(|i| (i, i * 2)).collect();
                assert_eq!(pairs, expected);
            }

            #[test]
            fn test_keys_and_values() {
                let mut m: $map_ty = $new;
                for i in 0..10i32 {
                    m.insert(i, i + 100);
                }
                let mut keys: Vec<_> = m.keys().copied().collect();
                keys.sort();
                assert_eq!(keys, (0..10i32).collect::<Vec<_>>());

                let mut vals: Vec<_> = m.values().copied().collect();
                vals.sort();
                assert_eq!(vals, (100..110i32).collect::<Vec<_>>());
            }

            #[test]
            fn test_resize_under_heavy_load() {
                let mut m: $map_ty =
                    <$map_ty as HashMapTrait<i32, i32>>::with_capacity_and_load_factor(4, 0.5);
                for i in 0..500i32 {
                    m.insert(i, i);
                }
                assert_eq!(m.len(), 500);
                for i in 0..500i32 {
                    assert_eq!(m.get(&i), Some(&i));
                }
            }

            #[test]
            fn test_interleaved_insert_remove() {
                let mut m: $map_ty = $new;
                for i in 0..100i32 {
                    m.insert(i, i);
                }
                for i in (1..100i32).step_by(2) {
                    m.remove(&i);
                }
                for i in (1..100i32).step_by(2) {
                    m.insert(i, i + 1000);
                }
                assert_eq!(m.len(), 100);
                for i in 0..100i32 {
                    if i % 2 == 0 {
                        assert_eq!(m.get(&i), Some(&i));
                    } else {
                        assert_eq!(m.get(&i), Some(&(i + 1000)));
                    }
                }
            }

            #[test]
            fn test_large_capacity() {
                let mut m: $map_ty =
                    <$map_ty as HashMapTrait<i32, i32>>::with_capacity(1024);
                for i in 0..1000i32 {
                    m.insert(i, i);
                }
                assert_eq!(m.len(), 1000);
                for i in 0..1000i32 {
                    assert_eq!(m.get(&i), Some(&i));
                }
            }
        }
    };
}

impl_tests!(chaining, ChainingHashMap<i32, i32>, ChainingHashMap::<i32, i32>::new());
impl_tests!(linear, LinearProbingHashMap<i32, i32>, LinearProbingHashMap::<i32, i32>::new());
impl_tests!(robinhood, RobinHoodHashMap<i32, i32>, RobinHoodHashMap::<i32, i32>::new());

// ---------------------------------------------------------------------------
// Collision-heavy hasher tests
// ---------------------------------------------------------------------------

mod collision_tests {
    use super::*;

    #[test]
    fn chaining_with_total_collision() {
        let mut m = ChainingHashMap::<i32, i32, CollisionHasherBuilder>::with_capacity_and_load_factor(
            16, 0.99,
        );
        for i in 0..50i32 {
            m.insert(i, i * 10);
        }
        assert_eq!(m.len(), 50);
        for i in 0..50i32 {
            assert_eq!(m.get(&i), Some(&(i * 10)));
        }
        for i in 0..25i32 {
            assert_eq!(m.remove(&i), Some(i * 10));
        }
        assert_eq!(m.len(), 25);
        for i in 25..50i32 {
            assert_eq!(m.get(&i), Some(&(i * 10)));
        }
    }

    #[test]
    fn linear_probing_with_mod_hasher() {
        let mut m = LinearProbingHashMap::<i32, i32, ModHasherBuilder>::with_capacity_and_load_factor(
            16, 0.75,
        );
        for i in 0..10i32 {
            m.insert(i, i);
        }
        assert_eq!(m.len(), 10);
        for i in 0..10i32 {
            assert_eq!(m.get(&i), Some(&i));
        }
        for i in 0..5i32 {
            m.remove(&i);
        }
        for i in 5..10i32 {
            assert_eq!(m.get(&i), Some(&i));
        }
    }

    #[test]
    fn robin_hood_with_mod_hasher() {
        let mut m = RobinHoodHashMap::<i32, i32, ModHasherBuilder>::with_capacity_and_load_factor(
            16, 0.75,
        );
        for i in 0..10i32 {
            m.insert(i, i);
        }
        assert_eq!(m.len(), 10);
        for i in 0..10i32 {
            assert_eq!(m.get(&i), Some(&i));
        }
        for i in 0..5i32 {
            m.remove(&i);
        }
        for i in 5..10i32 {
            assert_eq!(m.get(&i), Some(&i));
        }
    }

    #[test]
    fn robin_hood_probe_distance_bounded() {
        let mut m = RobinHoodHashMap::<i32, i32, ModHasherBuilder>::with_capacity_and_load_factor(
            32, 0.9,
        );
        for i in 0..25i32 {
            m.insert(i, i);
        }
        let max_dist = m.max_probe_distance();
        assert!(max_dist < 25, "Robin Hood max probe distance {} should be < 25", max_dist);
    }
}

// ---------------------------------------------------------------------------
// Cross-implementation consistency
// ---------------------------------------------------------------------------

mod consistency {
    use super::*;

    #[test]
    fn all_implementations_agree_on_results() {
        let mut c = ChainingHashMap::<i32, i32>::new();
        let mut l = LinearProbingHashMap::<i32, i32>::new();
        let mut r = RobinHoodHashMap::<i32, i32>::new();

        for i in 0..200i32 {
            c.insert(i, i * 7);
            l.insert(i, i * 7);
            r.insert(i, i * 7);
        }

        assert_eq!(c.len(), 200);
        assert_eq!(l.len(), 200);
        assert_eq!(r.len(), 200);

        for i in 0..200i32 {
            assert_eq!(c.get(&i), l.get(&i));
            assert_eq!(l.get(&i), r.get(&i));
        }

        for i in (0..200i32).step_by(3) {
            c.remove(&i);
            l.remove(&i);
            r.remove(&i);
        }

        for i in 0..200i32 {
            assert_eq!(c.get(&i), l.get(&i), "Mismatch at key {}", i);
            assert_eq!(l.get(&i), r.get(&i), "Mismatch at key {}", i);
        }
    }

    #[test]
    fn all_implementations_handle_empty_keys() {
        let c = ChainingHashMap::<i32, i32>::new();
        let l = LinearProbingHashMap::<i32, i32>::new();
        let r = RobinHoodHashMap::<i32, i32>::new();

        assert_eq!(c.get(&0i32), None);
        assert_eq!(l.get(&0i32), None);
        assert_eq!(r.get(&0i32), None);
    }
}

// ---------------------------------------------------------------------------
// Property-style tests (randomised)
// ---------------------------------------------------------------------------

mod property_tests {
    use super::*;
    use rand::prelude::*;
    use rand::rngs::StdRng;

    #[test]
    fn random_insert_remove_consistency() {
        let seed = 42u64;
        let mut rng = StdRng::seed_from_u64(seed);

        let mut c = ChainingHashMap::<i32, i32>::new();
        let mut l = LinearProbingHashMap::<i32, i32>::new();
        let mut r = RobinHoodHashMap::<i32, i32>::new();

        let mut reference = std::collections::HashMap::new();

        for _ in 0..5000 {
            let key: i32 = rng.gen_range(0..500);
            let op: u8 = rng.gen_range(0..3);

            match op {
                0 => {
                    let val: i32 = rng.gen_range(0..10000);
                    let cr = c.insert(key, val);
                    let lr = l.insert(key, val);
                    let rr = r.insert(key, val);
                    let refr = reference.insert(key, val);

                    assert_eq!(cr, lr, "chaining vs linear old value for key {}", key);
                    assert_eq!(lr, rr, "linear vs robinhood old value for key {}", key);
                    assert_eq!(cr, refr, "chaining vs reference old value for key {}", key);
                }
                1 => {
                    let cr = c.get(&key).copied();
                    let lr = l.get(&key).copied();
                    let rr = r.get(&key).copied();
                    let refr = reference.get(&key).copied();

                    assert_eq!(cr, lr, "chaining vs linear get for key {}", key);
                    assert_eq!(lr, rr, "linear vs robinhood get for key {}", key);
                    assert_eq!(cr, refr, "chaining vs reference get for key {}", key);
                }
                2 => {
                    let cr = c.remove(&key);
                    let lr = l.remove(&key);
                    let rr = r.remove(&key);
                    let refr = reference.remove(&key);

                    assert_eq!(cr, lr, "chaining vs linear remove for key {}", key);
                    assert_eq!(lr, rr, "linear vs robinhood remove for key {}", key);
                    assert_eq!(cr, refr, "chaining vs reference remove for key {}", key);
                }
                _ => unreachable!(),
            }
        }

        assert_eq!(c.len(), reference.len());
        assert_eq!(l.len(), reference.len());
        assert_eq!(r.len(), reference.len());
    }

    #[test]
    fn resize_never_loses_entries() {
        let mut rng = StdRng::seed_from_u64(123);
        let mut m = RobinHoodHashMap::<i32, i32>::with_capacity_and_load_factor(4, 0.5);

        for i in 0..500i32 {
            m.insert(i, i * 3);
        }
        for i in 0..250i32 {
            if rng.gen_bool(0.3) {
                m.remove(&i);
            }
        }
        for i in 0..500i32 {
            if m.contains_key(&i) {
                assert_eq!(m.get(&i), Some(&(i * 3)));
            }
        }
    }

    #[test]
    fn linear_probing_no_false_positives() {
        let mut m = LinearProbingHashMap::<i32, i32>::new();
        for i in (0..100i32).step_by(2) {
            m.insert(i, i);
        }
        for i in (1..100i32).step_by(2) {
            assert_eq!(m.get(&i), None, "False positive for key {}", i);
        }
        for i in (0..100i32).step_by(2) {
            m.remove(&i);
        }
        for i in 0..100i32 {
            assert_eq!(m.get(&i), None, "Key {} found after removal", i);
        }
    }

    #[test]
    fn robin_hood_no_false_positives() {
        let mut m = RobinHoodHashMap::<i32, i32>::new();
        for i in (0..100i32).step_by(2) {
            m.insert(i, i);
        }
        for i in (1..100i32).step_by(2) {
            assert_eq!(m.get(&i), None, "False positive for key {}", i);
        }
        for i in (0..100i32).step_by(2) {
            m.remove(&i);
        }
        for i in 0..100i32 {
            assert_eq!(m.get(&i), None, "Key {} found after removal", i);
        }
    }

    #[test]
    fn chaining_string_keys() {
        let mut m = ChainingHashMap::<String, i32>::new();
        m.insert("hello".to_string(), 1);
        m.insert("world".to_string(), 2);
        m.insert("rust".to_string(), 3);

        assert_eq!(m.get("hello"), Some(&1));
        assert_eq!(m.get("world"), Some(&2));
        assert_eq!(m.get("rust"), Some(&3));
        assert_eq!(m.get("missing"), None);

        m.remove("world");
        assert_eq!(m.get("world"), None);
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn robinhood_string_keys() {
        let mut m = RobinHoodHashMap::<String, i32>::new();
        m.insert("hello".to_string(), 1);
        m.insert("world".to_string(), 2);

        assert_eq!(m.get("hello"), Some(&1));
        assert_eq!(m.get("world"), Some(&2));

        m.remove("hello");
        assert_eq!(m.get("hello"), None);
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn linear_string_keys() {
        let mut m = LinearProbingHashMap::<String, i32>::new();
        m.insert("alpha".to_string(), 10);
        m.insert("beta".to_string(), 20);

        assert_eq!(m.get("alpha"), Some(&10));
        assert_eq!(m.get("beta"), Some(&20));

        m.remove("alpha");
        assert_eq!(m.get("alpha"), None);
        assert_eq!(m.len(), 1);
    }
}
