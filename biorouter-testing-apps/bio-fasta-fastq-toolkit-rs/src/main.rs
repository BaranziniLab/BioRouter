//! CLI entry point for bio-toolkit.

use std::io::{self, Read};
use std::fs::File;
use flate2::read::GzDecoder;
use clap::Parser;

use bio_fasta_fastq_toolkit::cli::{Cli, Command};
use bio_fasta_fastq_toolkit::fasta;
use bio_fasta_fastq_toolkit::fastq;
use bio_fasta_fastq_toolkit::stats;
use bio_fasta_fastq_toolkit::quality::{self, QualityEncoding};
use bio_fasta_fastq_toolkit::convert;
use bio_fasta_fastq_toolkit::seqops;

fn open_input(path: &str) -> Box<dyn Read> {
    if path == "-" {
        Box::new(io::stdin())
    } else if path.ends_with(".gz") {
        Box::new(GzDecoder::new(File::open(path).expect("Cannot open input file")))
    } else {
        Box::new(File::open(path).expect("Cannot open input file"))
    }
}

fn parse_encoding(s: &str) -> QualityEncoding {
    match s.to_lowercase().as_str() {
        "illumina" => QualityEncoding::Illumina,
        _ => QualityEncoding::Sanger,
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Stats { input, format } => {
            match format.to_lowercase().as_str() {
                "fasta" | "fa" | "fna" | "fas" => {
                    let iter = fasta::parse_reader(open_input(&input));
                    let records: Vec<_> = iter.collect::<Result<Vec<_>, _>>().expect("Parse error");
                    let sequences: Vec<&str> = records.iter().map(|r| r.sequence.as_str()).collect();
                    let lengths: Vec<usize> = sequences.iter().map(|s| s.len()).collect();
                    let ls = stats::length_stats(&lengths);
                    let comp = stats::aggregate_composition(&sequences);

                    println!("=== Sequence Statistics (FASTA) ===");
                    println!("Records:        {}", ls.count);
                    println!("Total bases:    {}", ls.total_bases);
                    println!("Min length:     {}", ls.min);
                    println!("Max length:     {}", ls.max);
                    println!("Mean length:    {:.1}", ls.mean);
                    println!("Median length:  {:.1}", ls.median);
                    println!("N50:            {}", ls.n50);
                    println!("L50:            {}", ls.l50);
                    println!();
                    println!("=== Base Composition ===");
                    println!("A: {} ({:.1}%)", comp.a, 100.0 * comp.a as f64 / comp.total().max(1) as f64);
                    println!("T: {} ({:.1}%)", comp.t, 100.0 * comp.t as f64 / comp.total().max(1) as f64);
                    println!("G: {} ({:.1}%)", comp.g, 100.0 * comp.g as f64 / comp.total().max(1) as f64);
                    println!("C: {} ({:.1}%)", comp.c, 100.0 * comp.c as f64 / comp.total().max(1) as f64);
                    println!("N: {} ({:.1}%)", comp.n, 100.0 * comp.n as f64 / comp.total().max(1) as f64);
                    println!("GC content:     {:.1}%", comp.gc_fraction() * 100.0);
                }
                "fastq" | "fq" => {
                    let iter = fastq::parse_reader(open_input(&input));
                    let records: Vec<_> = iter.collect::<Result<Vec<_>, _>>().expect("Parse error");
                    let sequences: Vec<&str> = records.iter().map(|r| r.sequence.as_str()).collect();
                    let lengths: Vec<usize> = sequences.iter().map(|s| s.len()).collect();
                    let ls = stats::length_stats(&lengths);
                    let comp = stats::aggregate_composition(&sequences);

                    println!("=== Sequence Statistics (FASTQ) ===");
                    println!("Records:        {}", ls.count);
                    println!("Total bases:    {}", ls.total_bases);
                    println!("Min length:     {}", ls.min);
                    println!("Max length:     {}", ls.max);
                    println!("Mean length:    {:.1}", ls.mean);
                    println!("Median length:  {:.1}", ls.median);
                    println!("N50:            {}", ls.n50);
                    println!("L50:            {}", ls.l50);
                    println!();
                    println!("=== Base Composition ===");
                    println!("A: {} ({:.1}%)", comp.a, 100.0 * comp.a as f64 / comp.total().max(1) as f64);
                    println!("T: {} ({:.1}%)", comp.t, 100.0 * comp.t as f64 / comp.total().max(1) as f64);
                    println!("G: {} ({:.1}%)", comp.g, 100.0 * comp.g as f64 / comp.total().max(1) as f64);
                    println!("C: {} ({:.1}%)", comp.c, 100.0 * comp.c as f64 / comp.total().max(1) as f64);
                    println!("N: {} ({:.1}%)", comp.n, 100.0 * comp.n as f64 / comp.total().max(1) as f64);
                    println!("GC content:     {:.1}%", comp.gc_fraction() * 100.0);
                }
                other => {
                    eprintln!("Unsupported format: {}", other);
                    std::process::exit(1);
                }
            }
        }

        Command::Filter { input, min_qual, encoding, output } => {
            let enc = parse_encoding(&encoding);
            let iter = fastq::parse_reader(open_input(&input));
            let records: Vec<_> = iter.collect::<Result<Vec<_>, _>>().expect("Parse error");
            let before = records.len();
            let filtered = quality::filter_by_quality(records, min_qual, enc).expect("Quality error");
            match output {
                Some(path) => {
                    use std::io::Write;
                    let mut file = File::create(&path).expect("Cannot create output file");
                    for rec in &filtered {
                        writeln!(file, ">{}", rec.id).expect("Write error");
                        writeln!(file, "{}", rec.sequence).expect("Write error");
                    }
                }
                None => {
                    let stdout = io::stdout();
                    let mut lock = stdout.lock();
                    for rec in &filtered {
                        convert::write_fasta_record(&mut lock, &rec.to_fasta()).expect("Write error");
                    }
                }
            }
            eprintln!("Kept {}/{} records (min mean quality: {})", filtered.len(), before, min_qual);
        }

        Command::Trim { input, window_size, min_qual, encoding, output } => {
            let enc = parse_encoding(&encoding);
            let iter = fastq::parse_reader(open_input(&input));
            let records: Vec<_> = iter.collect::<Result<Vec<_>, _>>().expect("Parse error");
            let before = records.len();
            let trimmed = quality::trim_records(records, window_size, min_qual, enc).expect("Trim error");
            match output {
                Some(path) => {
                    use std::io::Write;
                    let mut file = File::create(&path).expect("Cannot create output file");
                    for rec in &trimmed {
                        writeln!(file, "@{}", rec.id).expect("Write error");
                        writeln!(file, "{}", rec.sequence).expect("Write error");
                        writeln!(file, "+").expect("Write error");
                        writeln!(file, "{}", rec.quality).expect("Write error");
                    }
                }
                None => {
                    use std::io::Write;
                    let stdout = io::stdout();
                    let mut lock = stdout.lock();
                    for rec in &trimmed {
                        writeln!(lock, "@{}", rec.id).expect("Write error");
                        writeln!(lock, "{}", rec.sequence).expect("Write error");
                        writeln!(lock, "+").expect("Write error");
                        writeln!(lock, "{}", rec.quality).expect("Write error");
                    }
                }
            }
            eprintln!("Kept {}/{} records after trimming", trimmed.len(), before);
        }

        Command::Convert { input, output } => {
            let reader = open_input(&input);
            match output {
                Some(path) => {
                    let file = File::create(&path).expect("Cannot create output file");
                    let count = convert::fastq_to_fasta(reader, file).expect("Conversion error");
                    eprintln!("Converted {} records", count);
                }
                None => {
                    let stdout = io::stdout();
                    let count = convert::fastq_to_fasta(reader, stdout.lock()).expect("Conversion error");
                    eprintln!("Converted {} records", count);
                }
            }
        }

        Command::Subsample { input, fraction, format } => {
            match format.to_lowercase().as_str() {
                "fasta" | "fa" | "fna" | "fas" => {
                    let iter = fasta::parse_reader(open_input(&input));
                    let records: Vec<_> = iter.collect::<Result<Vec<_>, _>>().expect("Parse error");
                    let before = records.len();
                    let sampled = seqops::subsample(records, fraction);
                    let stdout = io::stdout();
                    let mut lock = stdout.lock();
                    for rec in &sampled {
                        convert::write_fasta_record(&mut lock, rec).expect("Write error");
                    }
                    eprintln!("Sampled {}/{} records", sampled.len(), before);
                }
                "fastq" | "fq" => {
                    let iter = fastq::parse_reader(open_input(&input));
                    let records: Vec<_> = iter.collect::<Result<Vec<_>, _>>().expect("Parse error");
                    let before = records.len();
                    let sampled = seqops::subsample(records, fraction);
                    let stdout = io::stdout();
                    let mut lock = stdout.lock();
                    use std::io::Write;
                    for rec in &sampled {
                        writeln!(lock, "@{}", rec.id).expect("Write error");
                        writeln!(lock, "{}", rec.sequence).expect("Write error");
                        writeln!(lock, "+").expect("Write error");
                        writeln!(lock, "{}", rec.quality).expect("Write error");
                    }
                    eprintln!("Sampled {}/{} records", sampled.len(), before);
                }
                other => {
                    eprintln!("Unsupported format: {}", other);
                    std::process::exit(1);
                }
            }
        }

        Command::Revcomp { input } => {
            let iter = fasta::parse_reader(open_input(&input));
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            let mut count = 0usize;
            for result in iter {
                let rec = result.expect("Parse error");
                let rc_seq = seqops::reverse_complement(&rec.sequence).expect("Invalid sequence");
                let rc_rec = fasta::FastaRecord {
                    id: rec.id,
                    description: rec.description,
                    sequence: rc_seq,
                };
                convert::write_fasta_record(&mut lock, &rc_rec).expect("Write error");
                count += 1;
            }
            eprintln!("Reverse-complemented {} records", count);
        }

        Command::Translate { input } => {
            let iter = fasta::parse_reader(open_input(&input));
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            let mut count = 0usize;
            for result in iter {
                let rec = result.expect("Parse error");
                let protein = seqops::translate(&rec.sequence).expect("Translation error");
                let prot_rec = fasta::FastaRecord {
                    id: format!("{}_protein", rec.id),
                    description: rec.description,
                    sequence: protein,
                };
                convert::write_fasta_record(&mut lock, &prot_rec).expect("Write error");
                count += 1;
            }
            eprintln!("Translated {} sequences", count);
        }
    }
}
