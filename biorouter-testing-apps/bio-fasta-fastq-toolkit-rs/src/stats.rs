//! Sequence statistics: length distribution, GC content, N50/L50, base composition.

/// Summary statistics for a collection of sequence lengths.
#[derive(Debug, Clone)]
pub struct LengthStats {
    pub count: usize,
    pub total_bases: usize,
    pub min: usize,
    pub max: usize,
    pub mean: f64,
    pub median: f64,
    pub n50: usize,
    pub l50: usize,
}

/// Base composition counts.
#[derive(Debug, Clone, Default)]
pub struct BaseComposition {
    pub a: usize,
    pub t: usize,
    pub g: usize,
    pub c: usize,
    pub n: usize,
    pub other: usize,
}

impl BaseComposition {
    pub fn total(&self) -> usize {
        self.a + self.t + self.g + self.c + self.n + self.other
    }

    /// GC fraction (0.0–1.0). Returns 0.0 if total is 0.
    pub fn gc_fraction(&self) -> f64 {
        let total = self.total();
        if total == 0 { 0.0 } else { (self.g + self.c) as f64 / total as f64 }
    }
}

/// Compute base composition of a sequence string.
pub fn base_composition(seq: &str) -> BaseComposition {
    let mut comp = BaseComposition::default();
    for ch in seq.chars() {
        match ch {
            'A' => comp.a += 1,
            'T' => comp.t += 1,
            'G' => comp.g += 1,
            'C' => comp.c += 1,
            'N' => comp.n += 1,
            _ => comp.other += 1,
        }
    }
    comp
}

/// Compute length statistics and N50/L50 from a slice of sequence lengths.
pub fn length_stats(lengths: &[usize]) -> LengthStats {
    if lengths.is_empty() {
        return LengthStats {
            count: 0, total_bases: 0, min: 0, max: 0,
            mean: 0.0, median: 0.0, n50: 0, l50: 0,
        };
    }

    let count = lengths.len();
    let total_bases: usize = lengths.iter().sum();
    let min = *lengths.iter().min().unwrap();
    let max = *lengths.iter().max().unwrap();
    let mean = total_bases as f64 / count as f64;

    let mut sorted = lengths.to_vec();
    sorted.sort_unstable();
    let median = if count % 2 == 0 {
        (sorted[count / 2 - 1] + sorted[count / 2]) as f64 / 2.0
    } else {
        sorted[count / 2] as f64
    };

    // N50: shortest sequence length such that sequences >= that length cover >= 50% of total
    let half = total_bases as f64 / 2.0;
    let mut cumulative = 0usize;
    let mut n50 = 0usize;
    let mut l50 = 0usize;
    // sorted ascending; walk from largest
    for (i, &len) in sorted.iter().rev().enumerate() {
        cumulative += len;
        if cumulative as f64 >= half {
            n50 = len;
            l50 = i + 1;
            break;
        }
    }

    LengthStats { count, total_bases, min, max, mean, median, n50, l50 }
}

/// Convenience: compute length stats from records that have a `len()` method.
pub fn length_stats_from_records<L: AsRef<str>>(sequences: &[L]) -> LengthStats {
    let lengths: Vec<usize> = sequences.iter().map(|s| s.as_ref().len()).collect();
    length_stats(&lengths)
}

/// Aggregate base composition across multiple sequences.
pub fn aggregate_composition(sequences: &[&str]) -> BaseComposition {
    let mut agg = BaseComposition::default();
    for seq in sequences {
        let c = base_composition(seq);
        agg.a += c.a;
        agg.t += c.t;
        agg.g += c.g;
        agg.c += c.c;
        agg.n += c.n;
        agg.other += c.other;
    }
    agg
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_composition_basic() {
        let comp = base_composition("ACGTACGT");
        assert_eq!(comp.a, 2);
        assert_eq!(comp.t, 2);
        assert_eq!(comp.g, 2);
        assert_eq!(comp.c, 2);
        assert_eq!(comp.n, 0);
        assert!((comp.gc_fraction() - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_base_composition_with_n() {
        let comp = base_composition("ACNGTN");
        assert_eq!(comp.n, 2);
        assert_eq!(comp.total(), 6);
    }

    #[test]
    fn test_base_composition_empty() {
        let comp = base_composition("");
        assert_eq!(comp.total(), 0);
        assert!((comp.gc_fraction()).abs() < 1e-10);
    }

    #[test]
    fn test_length_stats_basic() {
        let lengths = vec![100, 200, 300, 400, 500];
        let stats = length_stats(&lengths);
        assert_eq!(stats.count, 5);
        assert_eq!(stats.total_bases, 1500);
        assert_eq!(stats.min, 100);
        assert_eq!(stats.max, 500);
        assert!((stats.mean - 300.0).abs() < 1e-10);
        assert!((stats.median - 300.0).abs() < 1e-10);
        // N50: 500+400 = 900 >= 750 → N50 = 400
        assert_eq!(stats.n50, 400);
        assert_eq!(stats.l50, 2);
    }

    #[test]
    fn test_length_stats_empty() {
        let stats = length_stats(&[]);
        assert_eq!(stats.count, 0);
        assert_eq!(stats.n50, 0);
    }

    #[test]
    fn test_length_stats_single() {
        let stats = length_stats(&[1000]);
        assert_eq!(stats.n50, 1000);
        assert_eq!(stats.l50, 1);
        assert_eq!(stats.median, 1000.0);
    }

    #[test]
    fn test_n50_even_number() {
        // Two sequences: 100, 200. Total = 300, half = 150.
        // Sorted desc: 200 (cum=200 >= 150) → N50=200, L50=1
        let stats = length_stats(&[100, 200]);
        assert_eq!(stats.n50, 200);
        assert_eq!(stats.l50, 1);
    }

    #[test]
    fn test_aggregate_composition() {
        let comp = aggregate_composition(&["ACGT", "TTTT"]);
        assert_eq!(comp.a, 1);
        assert_eq!(comp.t, 5);
        assert_eq!(comp.g, 1);
        assert_eq!(comp.c, 1);
        assert_eq!(comp.total(), 8);
    }
}
