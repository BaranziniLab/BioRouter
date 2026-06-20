//! Seed finding: extract query k-mers and look them up in the database index.
//!
//! A "seed" is an exact k-mer match between a query position and a database position.
//! The seed-and-extend paradigm uses these as starting points for alignment extension.

use crate::index::KmerIndex;

/// A seed hit: query position matched to a database position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedHit {
    /// Database sequence index.
    pub db_seq_idx: usize,
    /// Database position (0-based).
    pub db_pos: usize,
    /// Query position (0-based).
    pub query_pos: usize,
}

/// Find all seed hits between a query sequence and a k-mer index.
///
/// For each k-mer window in the query, look it up in the index and record
/// all matching database positions as seed hits.
pub fn find_seeds(query: &[u8], index: &KmerIndex) -> Vec<SeedHit> {
    let k = index.k;
    if query.len() < k {
        return Vec::new();
    }

    let mut hits = Vec::new();

    for q_pos in 0..=(query.len() - k) {
        let kmer = &query[q_pos..q_pos + k];
        let db_hits = index.lookup(kmer);
        for db_hit in db_hits {
            hits.push(SeedHit {
                db_seq_idx: db_hit.seq_idx,
                db_pos: db_hit.pos,
                query_pos: q_pos,
            });
        }
    }

    hits
}

/// Find seed hits with ambiguity support (N/X in query treated as wildcards).
pub fn find_seeds_ambiguous(query: &[u8], index: &KmerIndex) -> Vec<SeedHit> {
    let k = index.k;
    if query.len() < k {
        return Vec::new();
    }

    let mut hits = Vec::new();

    for q_pos in 0..=(query.len() - k) {
        let kmer = &query[q_pos..q_pos + k];
        let db_hits = index.lookup_with_ambiguity(kmer);
        for db_hit in db_hits {
            hits.push(SeedHit {
                db_seq_idx: db_hit.seq_idx,
                db_pos: db_hit.pos,
                query_pos: q_pos,
            });
        }
    }

    hits
}

/// Cluster overlapping/diagonal seed hits to reduce redundancy.
///
/// Seeds that are close in both query and database coordinates are likely
/// part of the same alignment region. This groups them to avoid redundant
/// extension work.
pub fn cluster_seeds(hits: &[SeedHit], max_diagonal_distance: i32) -> Vec<Vec<&SeedHit>> {
    if hits.is_empty() {
        return Vec::new();
    }

    // Sort by (db_seq_idx, db_pos, query_pos)
    let mut sorted: Vec<&SeedHit> = hits.iter().collect();
    sorted.sort_by_key(|h| (h.db_seq_idx, h.db_pos, h.query_pos));

    let mut clusters: Vec<Vec<&SeedHit>> = Vec::new();
    let mut current_cluster: Vec<&SeedHit> = vec![sorted[0]];

    for hit in sorted.iter().skip(1) {
        let last = current_cluster.last().unwrap();
        // Same sequence and diagonal distance within threshold?
        let diag_dist = ((hit.db_pos as i32 - hit.query_pos as i32)
            - (last.db_pos as i32 - last.query_pos as i32))
            .abs();

        if hit.db_seq_idx == last.db_seq_idx && diag_dist <= max_diagonal_distance {
            current_cluster.push(hit);
        } else {
            clusters.push(std::mem::take(&mut current_cluster));
            current_cluster = vec![hit];
        }
    }
    clusters.push(current_cluster);

    clusters
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fasta::FastaRecord;

    fn make_records(seqs: &[(&str, &str)]) -> Vec<FastaRecord> {
        seqs.iter()
            .map(|(hdr, seq)| FastaRecord {
                header: hdr.to_string(),
                seq: seq.as_bytes().to_vec(),
            })
            .collect()
    }

    #[test]
    fn test_find_seeds_exact_match() {
        let recs = make_records(&[("db1", "ACGTACGT")]);
        let idx = KmerIndex::build(&recs, 4);
        let query = b"ACGTACGT";
        let seeds = find_seeds(query, &idx);

        // Query "ACGT" at pos 0 matches db at pos 0 and pos 4
        // Query "CGTA" at pos 1 matches db at pos 1
        // etc.
        assert!(!seeds.is_empty());

        // Check we have a seed at (db=0, query=0)
        assert!(seeds.contains(&SeedHit {
            db_seq_idx: 0,
            db_pos: 0,
            query_pos: 0,
        }));
    }

    #[test]
    fn test_find_seeds_no_match() {
        let recs = make_records(&[("db1", "TTTTTTTT")]);
        let idx = KmerIndex::build(&recs, 4);
        let query = b"ACGTACGT";
        let seeds = find_seeds(query, &idx);
        assert!(seeds.is_empty());
    }

    #[test]
    fn test_find_seeds_partial_overlap() {
        let recs = make_records(&[("db1", "ACGTACGT")]);
        let idx = KmerIndex::build(&recs, 4);
        let query = b"XXACGTXX";
        let seeds = find_seeds(query, &idx);
        // Only "ACGT" at query pos 2 should match
        let matching_seeds: Vec<_> = seeds.iter().filter(|s| s.query_pos == 2).collect();
        assert!(matching_seeds.len() >= 1);
    }

    #[test]
    fn test_cluster_seeds() {
        let hits = vec![
            SeedHit { db_seq_idx: 0, db_pos: 0, query_pos: 0 },
            SeedHit { db_seq_idx: 0, db_pos: 4, query_pos: 4 },
            SeedHit { db_seq_idx: 0, db_pos: 20, query_pos: 20 },
            SeedHit { db_seq_idx: 1, db_pos: 0, query_pos: 0 },
        ];

        let clusters = cluster_seeds(&hits, 5);
        // First three are same seq + same diagonal => one cluster
        // Fourth is different seq => separate cluster
        assert_eq!(clusters.len(), 2);
        assert_eq!(clusters[0].len(), 3);
        assert_eq!(clusters[1].len(), 1);
    }

    #[test]
    fn test_find_seeds_query_too_short() {
        let recs = make_records(&[("db1", "ACGTACGT")]);
        let idx = KmerIndex::build(&recs, 4);
        let query = b"AC";
        let seeds = find_seeds(query, &idx);
        assert!(seeds.is_empty());
    }

    #[test]
    fn test_find_seeds_multiple_db_seqs() {
        let recs = make_records(&[("db1", "ACGTACGT"), ("db2", "ACGTACGT")]);
        let idx = KmerIndex::build(&recs, 4);
        let query = b"ACGT";
        let seeds = find_seeds(query, &idx);
        // Should hit both sequences
        let seq0_hits = seeds.iter().filter(|s| s.db_seq_idx == 0).count();
        let seq1_hits = seeds.iter().filter(|s| s.db_seq_idx == 1).count();
        assert!(seq0_hits > 0);
        assert!(seq1_hits > 0);
    }
}
