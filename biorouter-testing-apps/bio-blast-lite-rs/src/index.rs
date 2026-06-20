//! K-mer index for database sequences.
//!
//! Builds an inverted index mapping each k-mer to its occurrences in the
//! database (sequence id, position). This enables O(1) lookup for seed hits.

use crate::fasta::FastaRecord;
use std::collections::HashMap;

/// Occurrence of a k-mer in the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KmerHit {
    /// Sequence index in the database.
    pub seq_idx: usize,
    /// Position within the sequence (0-based start).
    pub pos: usize,
}

/// K-mer index for a set of sequences.
#[derive(Debug, Clone)]
pub struct KmerIndex {
    /// Map from k-mer (as bytes) to list of hits.
    index: HashMap<Vec<u8>, Vec<KmerHit>>,
    /// Word size (k).
    pub k: usize,
    /// Number of indexed sequences.
    pub num_sequences: usize,
}

impl KmerIndex {
    /// Build a k-mer index from a set of sequences.
    pub fn build(records: &[FastaRecord], k: usize) -> Self {
        if k == 0 {
            panic!("k-mer size must be > 0");
        }

        let mut index: HashMap<Vec<u8>, Vec<KmerHit>> = HashMap::new();
        let mut total_kmers = 0usize;

        for (seq_idx, rec) in records.iter().enumerate() {
            if rec.len() < k {
                continue;
            }
            for pos in 0..=(rec.len() - k) {
                let kmer = rec.seq[pos..pos + k].to_vec();
                index
                    .entry(kmer)
                    .or_insert_with(Vec::new)
                    .push(KmerHit { seq_idx, pos });
                total_kmers += 1;
            }
        }

        eprintln!(
            "[index] Built k-mer index: k={}, sequences={}, indexed k-mers={}, unique k-mers={}",
            k,
            records.len(),
            total_kmers,
            index.len()
        );

        Self {
            index,
            k,
            num_sequences: records.len(),
        }
    }

    /// Look up a k-mer and return all hits.
    pub fn lookup(&self, kmer: &[u8]) -> &[KmerHit] {
        match self.index.get(kmer) {
            Some(hits) => hits,
            None => &[],
        }
    }

    /// Look up a k-mer, treating ambiguous positions (N, X) as wildcards.
    /// Returns all hits for any concrete k-mer that matches the pattern.
    pub fn lookup_with_ambiguity(&self, kmer: &[u8]) -> Vec<KmerHit> {
        // If no ambiguity, just do exact lookup
        let has_ambiguity = kmer.iter().any(|&b| b == b'N' || b == b'X');
        if !has_ambiguity {
            return self.lookup(kmer).to_vec();
        }

        // For small k, enumerate all possibilities
        if kmer.len() <= 12 {
            self.enumerate_ambiguous(kmer, 0, vec![], &mut Vec::new())
        } else {
            // For large k with ambiguity, just try the given kmer as-is
            self.lookup(kmer).to_vec()
        }
    }

    fn enumerate_ambiguous(
        &self,
        kmer: &[u8],
        pos: usize,
        mut current: Vec<u8>,
        results: &mut Vec<KmerHit>,
    ) -> Vec<KmerHit> {
        if pos == kmer.len() {
            let hits = self.lookup(&current);
            results.extend_from_slice(hits);
            return results.to_vec();
        }

        let bases: &[u8] = match kmer[pos] {
            b'N' | b'X' => b"ACGT",
            other => {
                current.push(other);
                let r = self.enumerate_ambiguous(kmer, pos + 1, current, results);
                return r;
            }
        };

        for &b in bases {
            let mut next = current.clone();
            next.push(b);
            self.enumerate_ambiguous(kmer, pos + 1, next, results);
        }

        results.to_vec()
    }

    /// Number of unique k-mers in the index.
    pub fn num_unique_kmers(&self) -> usize {
        self.index.len()
    }

    /// Number of total k-mer occurrences.
    pub fn total_hits(&self) -> usize {
        self.index.values().map(|v| v.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_records(seqs: &[(&str, &str)]) -> Vec<FastaRecord> {
        seqs.iter()
            .map(|(hdr, seq)| FastaRecord {
                header: hdr.to_string(),
                seq: seq.as_bytes().to_vec(),
            })
            .collect()
    }

    #[test]
    fn test_build_and_lookup() {
        let recs = make_records(&[("s1", "ACGTACGT"), ("s2", "TTTTACGT")]);
        let idx = KmerIndex::build(&recs, 4);

        // "ACGT" appears at s1:0, s1:4, s2:4
        let hits = idx.lookup(b"ACGT");
        assert_eq!(hits.len(), 3);
    }

    #[test]
    fn test_lookup_miss() {
        let recs = make_records(&[("s1", "AAAA")]);
        let idx = KmerIndex::build(&recs, 4);
        let hits = idx.lookup(b"TTTT");
        assert!(hits.is_empty());
    }

    #[test]
    fn test_kmers_too_short() {
        let recs = make_records(&[("s1", "AC")]);
        let idx = KmerIndex::build(&recs, 4);
        assert_eq!(idx.total_hits(), 0);
    }

    #[test]
    fn test_single_base_kmer() {
        let recs = make_records(&[("s1", "ACGT")]);
        let idx = KmerIndex::build(&recs, 1);
        assert_eq!(idx.total_hits(), 4);
    }

    #[test]
    fn test_ambiguity_lookup() {
        let recs = make_records(&[("s1", "ACGTACGT")]);
        let idx = KmerIndex::build(&recs, 4);
        // "ACGN" should match "ACGA", "ACGC", "ACGG", "ACGT"
        let hits = idx.lookup_with_ambiguity(b"ACGN");
        assert_eq!(hits.len(), 2); // "ACGT" appears at pos 0 and pos 4
    }

    #[test]
    fn test_index_stats() {
        let recs = make_records(&[("s1", "ACGT"), ("s2", "AAAA")]);
        let idx = KmerIndex::build(&recs, 2);
        assert_eq!(idx.num_sequences, 2);
        assert!(idx.num_unique_kmers() > 0);
    }
}
