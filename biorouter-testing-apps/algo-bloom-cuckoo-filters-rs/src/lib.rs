//! Probabilistic data structures library in Rust.
//!
//! Provides Bloom filter, Counting Bloom filter, Cuckoo filter,
//! and Scalable Bloom filter implementations, along with empirical
//! analysis utilities and benchmarking tools.

pub mod hashing;
pub mod bloom;
pub mod counting;
pub mod cuckoo;
pub mod scalable;
pub mod analysis;

pub use bloom::BloomFilter;
pub use counting::CountingBloomFilter;
pub use cuckoo::CuckooFilter;
pub use scalable::ScalableBloomFilter;
pub use hashing::{DefaultBuildHasher, DoubleHasher};
