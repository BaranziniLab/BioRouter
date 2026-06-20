//! Sequence operations: reverse complement, translation, subsampling.

use rand::Rng;
use crate::error::BioError;

/// Complement a single DNA base.
pub fn complement(base: char) -> Result<char, BioError> {
    match base {
        'A' => Ok('T'),
        'T' => Ok('A'),
        'G' => Ok('C'),
        'C' => Ok('G'),
        'N' => Ok('N'),
        'a' => Ok('t'),
        't' => Ok('a'),
        'g' => Ok('c'),
        'c' => Ok('g'),
        'n' => Ok('n'),
        other => Err(BioError::InvalidSequence { char: other, position: 0 }),
    }
}

/// Reverse complement of a DNA sequence.
pub fn reverse_complement(seq: &str) -> Result<String, BioError> {
    seq.chars().rev().map(|c| complement(c)).collect()
}

// Standard codon table (subset for DNA→protein translation).
fn codon_to_aa(codon: &str) -> char {
    match codon {
        "TTT" | "TTC" => 'F',
        "TTA" | "TTG" | "CTT" | "CTC" | "CTA" | "CTG" => 'L',
        "ATT" | "ATC" | "ATA" => 'I',
        "ATG" => 'M',
        "GTT" | "GTC" | "GTA" | "GTG" => 'V',
        "TCT" | "TCC" | "TCA" | "TCG" | "AGT" | "AGC" => 'S',
        "CCT" | "CCC" | "CCA" | "CCG" => 'P',
        "ACT" | "ACC" | "ACA" | "ACG" => 'T',
        "GCT" | "GCC" | "GCA" | "GCG" => 'A',
        "TAT" | "TAC" => 'Y',
        "TAA" | "TAG" | "TGA" => '*',
        "CAT" | "CAC" => 'H',
        "CAA" | "CAG" => 'Q',
        "AAT" | "AAC" => 'N',
        "AAA" | "AAG" => 'K',
        "GAT" | "GAC" => 'D',
        "GAA" | "GAG" => 'E',
        "TGT" | "TGC" => 'C',
        "TGG" => 'W',
        "CGT" | "CGC" | "CGA" | "CGG" | "AGA" | "AGG" => 'R',
        "GGT" | "GGC" | "GGA" | "GGG" => 'G',
        _ => 'X', // unknown codon (contains N or other)
    }
}

/// Translate a DNA sequence to protein (single-letter amino acid codes).
/// Reads the first complete codons; any trailing incomplete bases are ignored.
/// Stops at the first stop codon (`*`).
pub fn translate(seq: &str) -> Result<String, BioError> {
    let upper = seq.to_uppercase();
    let mut protein = String::new();
    for chunk in upper.as_bytes().chunks(3) {
        if chunk.len() < 3 {
            break;
        }
        let codon = std::str::from_utf8(chunk).unwrap_or("NNN");
        let aa = codon_to_aa(codon);
        if aa == '*' {
            break;
        }
        protein.push(aa);
    }
    Ok(protein)
}

/// Randomly subsample records from a vector, returning approximately `fraction` of them.
/// `fraction` should be in (0.0, 1.0].
pub fn subsample<T>(items: Vec<T>, fraction: f64) -> Vec<T> {
    if fraction <= 0.0 {
        return Vec::new();
    }
    if fraction >= 1.0 {
        return items;
    }
    let mut rng = rand::thread_rng();
    let mut out = Vec::new();
    for item in items {
        if rng.gen_bool(fraction.min(1.0)) {
            out.push(item);
        }
    }
    out
}

/// Subsample by exact count: randomly select exactly `n` items without replacement.
/// If `n >= items.len()`, returns all items.
pub fn subsample_exact<T>(items: Vec<T>, n: usize) -> Vec<T> {
    if n >= items.len() {
        return items;
    }
    let mut rng = rand::thread_rng();
    let mut pool: Vec<(usize, T)> = items.into_iter().enumerate().collect();
    let mut selected: Vec<(usize, T)> = Vec::with_capacity(n);
    for _ in 0..n {
        let idx = rng.gen_range(0..pool.len());
        let item = pool.swap_remove(idx);
        selected.push(item);
    }
    // Restore original order (by original index)
    selected.sort_by(|a, b| a.0.cmp(&b.0));
    selected.into_iter().map(|(_, v)| v).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reverse_complement() {
        assert_eq!(reverse_complement("ACGT").unwrap(), "ACGT");
        assert_eq!(reverse_complement("AAAA").unwrap(), "TTTT");
        assert_eq!(reverse_complement("A").unwrap(), "T");
        assert_eq!(reverse_complement("ATCG").unwrap(), "CGAT");
    }

    #[test]
    fn test_reverse_complement_lowercase() {
        assert_eq!(reverse_complement("acgt").unwrap(), "acgt");
    }

    #[test]
    fn test_reverse_complement_n() {
        assert_eq!(reverse_complement("ACNGT").unwrap(), "ACNGT");
    }

    #[test]
    fn test_reverse_complement_invalid() {
        assert!(reverse_complement("ACXB").is_err());
    }

    #[test]
    fn test_translate_basic() {
        // ATG = M, GCT = A, GGT = G
        assert_eq!(translate("ATGGCTGGT").unwrap(), "MAG");
    }

    #[test]
    fn test_translate_stop_codon() {
        // ATG = M, TAA = stop
        assert_eq!(translate("ATGTAA").unwrap(), "M");
    }

    #[test]
    fn test_translate_partial_codon() {
        // Only 2 bases — no complete codon
        assert_eq!(translate("AT").unwrap(), "");
    }

    #[test]
    fn test_translate_with_n() {
        // NNN → X (unknown)
        let protein = translate("NNN").unwrap();
        assert_eq!(protein, "X");
    }

    #[test]
    fn test_translate_empty() {
        assert_eq!(translate("").unwrap(), "");
    }

    #[test]
    fn test_subsample_exact() {
        let items: Vec<i32> = (0..100).collect();
        let sampled = subsample_exact(items, 10);
        assert_eq!(sampled.len(), 10);
        // Should be unique and sorted
        for i in 1..sampled.len() {
            assert!(sampled[i] > sampled[i - 1]);
        }
    }

    #[test]
    fn test_subsample_exact_too_large() {
        let items = vec![1, 2, 3];
        let sampled = subsample_exact(items, 10);
        assert_eq!(sampled.len(), 3);
    }

    #[test]
    fn test_subsample_fraction_zero() {
        let items = vec![1, 2, 3];
        let sampled = subsample(items, 0.0);
        assert!(sampled.is_empty());
    }

    #[test]
    fn test_subsample_fraction_one() {
        let items = vec![1, 2, 3];
        let sampled = subsample(items, 1.0);
        assert_eq!(sampled.len(), 3);
    }
}
