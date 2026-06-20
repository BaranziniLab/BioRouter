//! Scoring schemes for nucleotide and protein alignment.
//!
//! Supports:
//! - Nucleotide: simple match/mismatch scoring.
//! - Protein: BLOSUM substitution matrices (loaded at compile time from embedded data).

use std::collections::HashMap;

/// A scoring scheme for aligning two residues.
pub trait ScoringScheme {
    /// Score for aligning two residues.
    fn score(&self, a: u8, b: u8) -> i32;
    /// Score for a gap (affine or linear).
    fn gap_open(&self) -> i32;
    /// Gap extension penalty (for affine gap model).
    fn gap_extend(&self) -> i32;
    /// Alphabet size (for E-value calculations).
    fn alphabet_size(&self) -> usize;
}

// ============================================================================
// Nucleotide scoring
// ============================================================================

/// Simple nucleotide match/mismatch scoring.
#[derive(Debug, Clone)]
pub struct NucleotideScoring {
    pub match_score: i32,
    pub mismatch_score: i32,
    pub gap_open_penalty: i32,
    pub gap_extend_penalty: i32,
}

impl Default for NucleotideScoring {
    fn default() -> Self {
        Self {
            match_score: 2,
            mismatch_score: -3,
            gap_open_penalty: 5,
            gap_extend_penalty: 2,
        }
    }
}

impl NucleotideScoring {
    pub fn new(match_score: i32, mismatch_score: i32) -> Self {
        Self {
            match_score,
            mismatch_score,
            gap_open_penalty: 5,
            gap_extend_penalty: 2,
        }
    }
}

impl ScoringScheme for NucleotideScoring {
    fn score(&self, a: u8, b: u8) -> i32 {
        if a == b {
            self.match_score
        } else {
            self.mismatch_score
        }
    }

    fn gap_open(&self) -> i32 {
        self.gap_open_penalty
    }

    fn gap_extend(&self) -> i32 {
        self.gap_extend_penalty
    }

    fn alphabet_size(&self) -> usize {
        5 // ACGT + N
    }
}

// ============================================================================
// BLOSUM matrix for protein sequences
// ============================================================================

/// A substitution matrix (e.g. BLOSUM62).
#[derive(Debug, Clone)]
pub struct SubstitutionMatrix {
    #[allow(dead_code)]
    name: String,
    /// Scores indexed by (aa1_idx * size + aa2_idx)
    scores: Vec<i32>,
    size: usize,
    aa_to_idx: HashMap<u8, usize>,
    gap_open_penalty: i32,
    gap_extend_penalty: i32,
}

impl SubstitutionMatrix {
    /// Create from an explicit score map and alphabet.
    pub fn new(
        name: &str,
        alphabet: &[u8],
        raw_scores: &[&[i32]],
        gap_open: i32,
        gap_extend: i32,
    ) -> Self {
        let size = alphabet.len();
        let mut aa_to_idx = HashMap::new();
        for (i, &aa) in alphabet.iter().enumerate() {
            aa_to_idx.insert(aa, i);
            aa_to_idx.insert(aa.to_ascii_uppercase(), i);
            aa_to_idx.insert(aa.to_ascii_lowercase(), i);
        }
        let scores: Vec<i32> = raw_scores.iter().flat_map(|row| row.iter().copied()).collect();
        Self {
            name: name.to_string(),
            scores,
            size,
            aa_to_idx,
            gap_open_penalty: gap_open,
            gap_extend_penalty: gap_extend,
        }
    }

    /// Get BLOSUM62 matrix (standard protein substitution matrix).
    pub fn blosum62() -> Self {
        let alphabet: &[u8] = b"ARNDCQEGHILKMFPSTWYV";
        // fmt: off
        let raw: Vec<Vec<i32>> = vec![
            vec![ 4,-1,-2,-2, 0,-1,-1, 0,-2,-1,-1,-1,-1,-2,-1, 1, 0,-3,-2, 0], // A
            vec![-1, 5, 0,-2,-3, 1, 0,-2, 0,-3,-2, 2,-1,-3,-2,-1,-1,-3,-2,-3], // R
            vec![-2, 0, 6, 1,-3, 0, 0, 0, 1,-3,-3, 0,-2,-3,-2, 1, 0,-4,-2,-3], // N
            vec![-2,-2, 1, 6,-3, 0, 2,-1,-1,-3,-4,-1,-3,-3,-1, 0,-1,-4,-3,-3], // D
            vec![ 0,-3,-3,-3, 9,-3,-4,-3,-3,-1,-1,-3,-1,-2,-3,-1,-1,-2,-2,-1], // C
            vec![-1, 1, 0, 0,-3, 5, 0,-2, 0,-3,-2, 1, 0,-3,-1, 0,-1,-2,-1,-2], // Q
            vec![-1, 0, 0, 2,-4, 0, 6,-2, 0,-3,-3, 0,-2,-3,-2, 0,-1,-3,-2,-3], // E
            vec![ 0,-2, 0,-1,-3,-2,-2, 6,-2,-4,-4,-2,-3,-3,-2, 0,-2,-2,-3,-3], // G
            vec![-2, 0, 1,-1,-3, 0, 0,-2, 8,-3,-3,-1,-2,-1,-2,-1,-2,-2, 2,-3], // H
            vec![-1,-3,-3,-3,-1,-3,-3,-4,-3, 4, 2,-3, 1, 0,-3,-2,-1,-3,-1, 3], // I
            vec![-1,-2,-3,-4,-1,-2,-3,-4,-3, 2, 4,-2, 2, 0,-3,-2,-1,-2,-1, 1], // L
            vec![-1, 2, 0,-1,-3, 1, 0,-2,-1,-3,-2, 5,-1,-3,-1, 0,-1,-3,-2,-3], // K
            vec![-1,-1,-2,-3,-1, 0,-2,-3,-2, 1, 2,-1, 5, 0,-2,-1,-1,-1,-1, 1], // M
            vec![-2,-3,-3,-3,-2,-3,-3,-3,-1, 0, 0,-3, 0, 6,-4,-2,-2, 1, 3,-1], // F
            vec![-1,-2,-2,-1,-3,-1,-2,-2,-2,-3,-3,-1,-2,-4, 7,-1,-1,-4,-3,-2], // P
            vec![ 1,-1, 1, 0,-1, 0, 0, 0,-1,-2,-2, 0,-1,-2,-1, 4, 1,-3,-2,-2], // S
            vec![ 0,-1, 0,-1,-1,-1,-1,-2,-2,-1,-1,-1,-1,-2,-1, 1, 5,-2,-2, 0], // T
            vec![-3,-3,-4,-4,-2,-2,-3,-2,-2,-3,-2,-3,-1, 1,-4,-3,-2,11, 2,-3], // W
            vec![-2,-2,-2,-3,-2,-1,-2,-3, 2,-1,-1,-2,-1, 3,-3,-2,-2, 2, 7,-1], // Y
            vec![ 0,-3,-3,-3,-1,-2,-3,-3,-3, 3, 1,-3, 1,-1,-2,-2, 0,-3,-1, 4], // V
        ];
        let scores_ref: Vec<&[i32]> = raw.iter().map(|v| v.as_slice()).collect();
        Self::new("BLOSUM62", alphabet, &scores_ref, 11, 1)
    }
}

impl ScoringScheme for SubstitutionMatrix {
    fn score(&self, a: u8, b: u8) -> i32 {
        let &i = self.aa_to_idx.get(&a).unwrap_or(&0);
        let &j = self.aa_to_idx.get(&b).unwrap_or(&0);
        self.scores[i * self.size + j]
    }

    fn gap_open(&self) -> i32 {
        self.gap_open_penalty
    }

    fn gap_extend(&self) -> i32 {
        self.gap_extend_penalty
    }

    fn alphabet_size(&self) -> usize {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nucleotide_match() {
        let scheme = NucleotideScoring::default();
        assert_eq!(scheme.score(b'A', b'A'), 2);
        assert_eq!(scheme.score(b'A', b'T'), -3);
        assert_eq!(scheme.score(b'C', b'G'), -3);
    }

    #[test]
    fn test_nucleotide_gap() {
        let scheme = NucleotideScoring::default();
        assert_eq!(scheme.gap_open(), 5);
        assert_eq!(scheme.gap_extend(), 2);
    }

    #[test]
    fn test_blosum62_self_score() {
        let mat = SubstitutionMatrix::blosum62();
        // Self-scores should be positive
        assert!(mat.score(b'A', b'A') > 0);
        assert!(mat.score(b'W', b'W') > 0);
        assert_eq!(mat.score(b'A', b'A'), 4);
    }

    #[test]
    fn test_blosum62_symmetry() {
        let mat = SubstitutionMatrix::blosum62();
        assert_eq!(mat.score(b'A', b'R'), mat.score(b'R', b'A'));
        assert_eq!(mat.score(b'D', b'E'), mat.score(b'E', b'D'));
    }

    #[test]
    fn test_blosum62_mismatch() {
        let mat = SubstitutionMatrix::blosum62();
        // W (Tryptophan) vs D (Aspartate) should be strongly negative
        assert!(mat.score(b'W', b'D') < 0);
        // W vs W is positive (self-score)
        assert!(mat.score(b'W', b'W') > 0);
    }

    #[test]
    fn test_custom_scoring() {
        let scheme = NucleotideScoring::new(1, -1);
        assert_eq!(scheme.score(b'A', b'A'), 1);
        assert_eq!(scheme.score(b'A', b'C'), -1);
    }
}
