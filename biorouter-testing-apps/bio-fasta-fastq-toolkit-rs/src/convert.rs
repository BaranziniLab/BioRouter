//! Format conversion: FASTQ → FASTA.

use std::io::{Read, Write, BufWriter};

use crate::error::BioError;
use crate::fastq;
use crate::fasta::FastaRecord;

/// Write a FastaRecord in FASTA format.
pub fn write_fasta_record<W: Write>(writer: &mut W, rec: &FastaRecord) -> Result<(), BioError> {
    if rec.description.is_empty() {
        writeln!(writer, ">{}", rec.id)?;
    } else {
        writeln!(writer, ">{} {}", rec.id, rec.description)?;
    }
    // Write sequence in lines of 80 characters (standard wrapping)
    for chunk in rec.sequence.as_bytes().chunks(80) {
        writer.write_all(chunk)?;
        writeln!(writer)?;
    }
    Ok(())
}

/// Convert a FASTQ stream to a FASTA stream.
pub fn fastq_to_fasta<R: Read, W: Write>(reader: R, writer: W) -> Result<usize, BioError> {
    let mut out = BufWriter::new(writer);
    let mut count = 0usize;
    for result in fastq::parse_reader(reader) {
        let rec = result?;
        let fasta = rec.to_fasta();
        write_fasta_record(&mut out, &fasta)?;
        count += 1;
    }
    out.flush()?;
    Ok(count)
}

/// Convert a FASTQ file to FASTA (writes to `out_path`).
pub fn convert_file(in_path: &str, out_path: &str) -> Result<usize, BioError> {
    let iter = fastq::parse_file(in_path)?;
    let mut out = BufWriter::new(std::fs::File::create(out_path)?);
    let mut count = 0usize;
    for result in iter {
        let rec = result?;
        let fasta = rec.to_fasta();
        write_fasta_record(&mut out, &fasta)?;
        count += 1;
    }
    out.flush()?;
    Ok(count)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fasta;

    #[test]
    fn test_fastq_to_fasta() {
        let input = b"@read1 desc\nACGT\n+\nIIII\n@read2\nTTTT\n+\n!!!!\n";
        let mut output = Vec::new();
        let count = fastq_to_fasta(&input[..], &mut output).unwrap();
        assert_eq!(count, 2);

        let fasta_str = String::from_utf8(output).unwrap();
        let records: Vec<_> = fasta::parse_reader(fasta_str.as_bytes())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].id, "read1");
        assert_eq!(records[0].description, "desc");
        assert_eq!(records[0].sequence, "ACGT");
        assert_eq!(records[1].id, "read2");
        assert_eq!(records[1].sequence, "TTTT");
    }

    #[test]
    fn test_fastq_to_fasta_empty() {
        let input = b"";
        let mut output = Vec::new();
        let count = fastq_to_fasta(&input[..], &mut output).unwrap();
        assert_eq!(count, 0);
        assert!(output.is_empty());
    }

    #[test]
    fn test_write_fasta_wrapping() {
        // Sequence > 80 chars should be wrapped.
        let long_seq = "A".repeat(200);
        let rec = FastaRecord {
            id: "long".into(),
            description: String::new(),
            sequence: long_seq.clone(),
        };
        let mut output = Vec::new();
        write_fasta_record(&mut output, &rec).unwrap();
        let s = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines[0], ">long");
        // First sequence line should be 80 chars, second 80, third 40
        assert_eq!(lines[1].len(), 80);
        assert_eq!(lines[2].len(), 80);
        assert_eq!(lines[3].len(), 40);
    }
}
