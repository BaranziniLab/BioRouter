//! FASTQ quality analysis: Phred decoding, per-base statistics, filtering and trimming.

use crate::error::BioError;
use crate::fastq::FastqRecord;

/// Quality encoding scheme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QualityEncoding {
    /// Sanger / Illumina 1.8+ (Phred+33, ASCII 33–126)
    Sanger,
    /// Illumina 1.3–1.7 (Phred+64, ASCII 64–126)
    Illumina,
}

impl QualityEncoding {
    /// ASCII offset for this encoding.
    pub fn offset(&self) -> u8 {
        match self {
            QualityEncoding::Sanger => 33,
            QualityEncoding::Illumina => 64,
        }
    }
}

/// Decode a single ASCII quality character to a Phred score.
pub fn decode_phred(qual_char: u8, encoding: QualityEncoding) -> Result<u8, BioError> {
    let offset = encoding.offset();
    if qual_char < offset {
        return Err(BioError::InvalidQualityEncoding(format!(
            "Quality char '{}' (ASCII {}) is below offset {} for {:?}",
            qual_char as char, qual_char, offset, encoding
        )));
    }
    Ok(qual_char - offset)
}

/// Decode an entire quality string to Phred scores.
pub fn decode_quality_string(qual: &str, encoding: QualityEncoding) -> Result<Vec<u8>, BioError> {
    qual.bytes().map(|b| decode_phred(b, encoding)).collect()
}

/// Per-base quality statistics across a set of records.
#[derive(Debug, Clone)]
pub struct PerBaseQuality {
    /// Mean quality at each position.
    pub mean: Vec<f64>,
    /// Minimum quality at each position.
    pub min: Vec<u8>,
    /// Maximum quality at each position.
    pub max: Vec<u8>,
    /// Number of records contributing to each position.
    pub count: Vec<usize>,
}

/// Compute per-base mean quality across records.
pub fn per_base_quality(records: &[FastqRecord], encoding: QualityEncoding) -> Result<PerBaseQuality, BioError> {
    if records.is_empty() {
        return Ok(PerBaseQuality { mean: vec![], min: vec![], max: vec![], count: vec![] });
    }

    let max_len = records.iter().map(|r| r.quality.len()).max().unwrap_or(0);
    let mut sums = vec![0u64; max_len];
    let mut counts = vec![0usize; max_len];
    let mut mins = vec![u8::MAX; max_len];
    let mut maxs = vec![0u8; max_len];

    for rec in records {
        let scores = decode_quality_string(&rec.quality, encoding)?;
        for (i, &score) in scores.iter().enumerate() {
            sums[i] += score as u64;
            counts[i] += 1;
            if score < mins[i] { mins[i] = score; }
            if score > maxs[i] { maxs[i] = score; }
        }
    }

    let mean: Vec<f64> = sums.iter().zip(counts.iter()).map(|(&s, &c)| {
        if c == 0 { 0.0 } else { s as f64 / c as f64 }
    }).collect();

    Ok(PerBaseQuality { mean, min: mins, max: maxs, count: counts })
}

/// Average quality of a single quality string.
pub fn mean_quality(qual: &str, encoding: QualityEncoding) -> Result<f64, BioError> {
    let scores = decode_quality_string(qual, encoding)?;
    if scores.is_empty() {
        return Ok(0.0);
    }
    let sum: u64 = scores.iter().map(|&s| s as u64).sum();
    Ok(sum as f64 / scores.len() as f64)
}

// ---------------------------------------------------------------------------
// Filtering
// ---------------------------------------------------------------------------

/// Filter: keep only records whose mean quality >= `min_qual`.
pub fn filter_by_quality(records: Vec<FastqRecord>, min_qual: f64, encoding: QualityEncoding) -> Result<Vec<FastqRecord>, BioError> {
    let mut out = Vec::new();
    for rec in records {
        let mq = mean_quality(&rec.quality, encoding)?;
        if mq >= min_qual {
            out.push(rec);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Trimming (sliding window)
// ---------------------------------------------------------------------------

/// Trim a single record using a sliding-window quality approach.
/// Walks from the 3' end; once the mean quality in a window of `window_size`
/// falls below `min_qual`, trims from that position onward.
/// Returns the trimmed record (may be empty if the entire read is low quality).
pub fn trim_sliding_window(record: &FastqRecord, window_size: usize, min_qual: f64, encoding: QualityEncoding) -> Result<FastqRecord, BioError> {
    let scores = decode_quality_string(&record.quality, encoding)?;
    if window_size == 0 || scores.is_empty() {
        return Ok(record.clone());
    }

    let ws = window_size.min(scores.len());
    // Find the first position from the start where a window of `ws` has mean < min_qual.
    // We keep everything before that position.
    let mut trim_pos = scores.len(); // default: keep all

    for i in 0..=scores.len().saturating_sub(ws) {
        let window_sum: u64 = scores[i..i + ws].iter().map(|&s| s as u64).sum();
        let window_mean = window_sum as f64 / ws as f64;
        if window_mean < min_qual {
            trim_pos = i;
            break;
        }
    }

    Ok(FastqRecord {
        id: record.id.clone(),
        description: record.description.clone(),
        sequence: record.sequence[..trim_pos].to_string(),
        quality: record.quality[..trim_pos].to_string(),
    })
}

/// Trim a vector of records using a sliding window.
pub fn trim_records(records: Vec<FastqRecord>, window_size: usize, min_qual: f64, encoding: QualityEncoding) -> Result<Vec<FastqRecord>, BioError> {
    let mut out = Vec::with_capacity(records.len());
    for rec in records {
        let trimmed = trim_sliding_window(&rec, window_size, min_qual, encoding)?;
        if !trimmed.is_empty() {
            out.push(trimmed);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(seq: &str, qual: &str) -> FastqRecord {
        FastqRecord {
            id: "test".into(),
            description: String::new(),
            sequence: seq.into(),
            quality: qual.into(),
        }
    }

    #[test]
    fn test_decode_phred_sanger() {
        // '!' = ASCII 33 → Phred 0
        assert_eq!(decode_phred(b'!', QualityEncoding::Sanger).unwrap(), 0);
        // 'I' = ASCII 73 → Phred 40
        assert_eq!(decode_phred(b'I', QualityEncoding::Sanger).unwrap(), 40);
    }

    #[test]
    fn test_decode_phred_illumina() {
        // '@' = ASCII 64 → Phred 0
        assert_eq!(decode_phred(b'@', QualityEncoding::Illumina).unwrap(), 0);
        // 'h' = ASCII 104 → Phred 40
        assert_eq!(decode_phred(b'h', QualityEncoding::Illumina).unwrap(), 40);
    }

    #[test]
    fn test_decode_phred_invalid() {
        // ASCII 32 (space) is below Sanger offset 33
        assert!(decode_phred(b' ', QualityEncoding::Sanger).is_err());
    }

    #[test]
    fn test_decode_quality_string() {
        let scores = decode_quality_string("IIII", QualityEncoding::Sanger).unwrap();
        assert_eq!(scores, vec![40, 40, 40, 40]);
    }

    #[test]
    fn test_mean_quality() {
        let mq = mean_quality("IIII", QualityEncoding::Sanger).unwrap();
        assert!((mq - 40.0).abs() < 1e-10);

        let mq2 = mean_quality("!!!!", QualityEncoding::Sanger).unwrap();
        assert!((mq2 - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_filter_by_quality() {
        let records = vec![
            make_record("ACGT", "IIII"),  // mean=40
            make_record("ACGT", "!!!!"),  // mean=0
            make_record("ACGT", "BBBB"),  // mean=33 (B = ASCII 66, Phred 33)
        ];
        let filtered = filter_by_quality(records, 20.0, QualityEncoding::Sanger).unwrap();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_per_base_quality() {
        let records = vec![
            make_record("ACGT", "IIII"),
            make_record("ACGT", "!!!!"),
        ];
        let pbq = per_base_quality(&records, QualityEncoding::Sanger).unwrap();
        assert_eq!(pbq.mean.len(), 4);
        for m in &pbq.mean {
            assert!((m - 20.0).abs() < 1e-10); // (40+0)/2
        }
        assert_eq!(pbq.min, vec![0, 0, 0, 0]);
        assert_eq!(pbq.max, vec![40, 40, 40, 40]);
    }

    #[test]
    fn test_trim_sliding_window() {
        // Window of 4, threshold 20. Quality starts good, ends bad.
        // 'I'=40, '!'=0
        let rec = make_record("ACGTACGT", "III!!!I!");
        let trimmed = trim_sliding_window(&rec, 4, 20.0, QualityEncoding::Sanger).unwrap();
        // Window starting at 0: [40,40,40,0] mean=30 ≥ 20 → keep
        // Window starting at 1: [40,40,0,0] mean=20 ≥ 20 → keep
        // Window starting at 2: [40,0,0,0] mean=10 < 20 → trim at pos 2
        assert_eq!(trimmed.sequence, "AC");
        assert_eq!(trimmed.quality, "II");
    }

    #[test]
    fn test_trim_entire_read_low_quality() {
        let rec = make_record("ACGT", "!!!!");
        let trimmed = trim_sliding_window(&rec, 4, 20.0, QualityEncoding::Sanger).unwrap();
        assert!(trimmed.is_empty());
    }

    #[test]
    fn test_trim_all_good() {
        let rec = make_record("ACGT", "IIII");
        let trimmed = trim_sliding_window(&rec, 4, 20.0, QualityEncoding::Sanger).unwrap();
        assert_eq!(trimmed.sequence, "ACGT");
    }

    #[test]
    fn test_per_base_quality_empty() {
        let pbq = per_base_quality(&[], QualityEncoding::Sanger).unwrap();
        assert!(pbq.mean.is_empty());
    }
}
