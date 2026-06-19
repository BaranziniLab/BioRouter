//! # algo-hash-table-impl-rs
//!
//! Hash table library implementing multiple collision-resolution strategies:
//!
//! - **Chaining** — separate chains per bucket (`Vec<(K, V)>` per slot)
//! - **Linear Probing** — open addressing with tombstone deletion
//! - **Robin Hood Hashing** — open addressing with displacement-based swaps
//!   and backward-shift deletion
//!
//! All implementations are generic over `<K, V, S>` (key, value, hasher)
//! and expose a unified [`common::HashMap`] trait.
//!
//! # Quick Start
//!
//! ```
//! use algo_hash_table_impl_rs::chaining::ChainingHashMap;
//! use algo_hash_table_impl_rs::common::HashMap;
//!
//! let mut m = ChainingHashMap::new();
//! m.insert("key", 42);
//! assert_eq!(m.get("key"), Some(&42));
//! ```

pub mod chaining;
pub mod cluster_analysis;
pub mod common;
pub mod linear;
pub mod robinhood;
