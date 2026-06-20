"""
Command-line interface for the genome assembler.

Provides a unified CLI for:
- Assembling reads using OLC or DBG algorithms
- Simulating reads from a reference
- Computing assembly statistics
"""

from __future__ import annotations

import argparse
import os
import sys
import time
from typing import List, Optional

from . import __version__
from .io import SequenceRecord, read_sequences, write_fasta
from .metrics import AssemblyStats, compute_assembly_stats, compute_assembly_stats_from_records


def create_parser() -> argparse.ArgumentParser:
    """Create the argument parser for the CLI."""
    parser = argparse.ArgumentParser(
        prog="bioassembly",
        description="A mini de-novo genome assembler in pure Python",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Assemble reads using de Bruijn graph (default)
  bioassembly assemble -i reads.fastq -o contigs.fasta

  # Assemble using OLC algorithm
  bioassembly assemble -i reads.fasta -o contigs.fasta --method olc

  # Simulate reads from a reference
  bioassembly simulate -r reference.fasta -o reads.fastq -n 1000

  # Compute assembly statistics
  bioassembly stats -i contigs.fasta
        """,
    )
    
    parser.add_argument("--version", action="version", version=f"%(prog)s {__version__}")
    
    subparsers = parser.add_subparsers(dest="command", help="Available commands")
    
    # Assemble command
    assemble_parser = subparsers.add_parser(
        "assemble",
        help="Assemble reads into contigs",
        description="Assemble sequencing reads into contigs",
    )
    assemble_parser.add_argument(
        "-i", "--input",
        required=True,
        help="Input reads file (FASTA or FASTQ)",
    )
    assemble_parser.add_argument(
        "-o", "--output",
        required=True,
        help="Output contigs file (FASTA)",
    )
    assemble_parser.add_argument(
        "-m", "--method",
        choices=["dbg", "olc"],
        default="dbg",
        help="Assembly method (default: dbg)",
    )
    assemble_parser.add_argument(
        "-k", "--kmer-size",
        type=int,
        default=21,
        help="K-mer size for DBG (default: 21)",
    )
    assemble_parser.add_argument(
        "--min-overlap",
        type=int,
        default=500,
        help="Minimum overlap for OLC (default: 500)",
    )
    assemble_parser.add_argument(
        "--max-error-rate",
        type=float,
        default=0.1,
        help="Maximum error rate for overlaps (default: 0.1)",
    )
    assemble_parser.add_argument(
        "-v", "--verbose",
        action="store_true",
        help="Verbose output",
    )
    assemble_parser.add_argument(
        "--stats-file",
        help="Save assembly statistics to file",
    )
    
    # Simulate command
    simulate_parser = subparsers.add_parser(
        "simulate",
        help="Simulate reads from a reference",
        description="Generate simulated sequencing reads from a reference sequence",
    )
    simulate_parser.add_argument(
        "-r", "--reference",
        required=True,
        help="Reference sequence file (FASTA)",
    )
    simulate_parser.add_argument(
        "-o", "--output",
        required=True,
        help="Output reads file (FASTQ)",
    )
    simulate_parser.add_argument(
        "-n", "--num-reads",
        type=int,
        default=1000,
        help="Number of reads to simulate (default: 1000)",
    )
    simulate_parser.add_argument(
        "-l", "--read-length",
        type=int,
        default=150,
        help="Read length (default: 150)",
    )
    simulate_parser.add_argument(
        "--error-rate",
        type=float,
        default=0.001,
        help="Error rate per base (default: 0.001)",
    )
    simulate_parser.add_argument(
        "--seed",
        type=int,
        help="Random seed for reproducibility",
    )
    
    # Stats command
    stats_parser = subparsers.add_parser(
        "stats",
        help="Compute assembly statistics",
        description="Compute statistics for an assembled contig file",
    )
    stats_parser.add_argument(
        "-i", "--input",
        required=True,
        help="Input contigs file (FASTA)",
    )
    stats_parser.add_argument(
        "-o", "--output",
        help="Output statistics file (optional, prints to stdout if not specified)",
    )
    
    return parser


def cmd_assemble(args: argparse.Namespace) -> None:
    """Handle the assemble command."""
    from .dbg import DBGAssembler
    from .olc import OLCAssembler
    
    print(f"Reading input reads from {args.input}...")
    reads = read_sequences(args.input)
    print(f"Read {len(reads)} sequences")
    
    start_time = time.time()
    
    if args.method == "dbg":
        print(f"Assembling with De Bruijn Graph (k={args.kmer_size})...")
        assembler = DBGAssembler(
            k=args.kmer_size,
            min_coverage=0.1,
            max_tip_length=10,
        )
    else:
        print(f"Assembling with OLC (min_overlap={args.min_overlap})...")
        assembler = OLCAssembler(
            min_overlap=args.min_overlap,
            max_error_rate=args.max_error_rate,
        )
    
    contigs = assembler.assemble(reads)
    
    elapsed = time.time() - start_time
    print(f"Assembly completed in {elapsed:.2f} seconds")
    
    # Write output
    write_fasta(contigs, args.output)
    print(f"Wrote {len(contigs)} contigs to {args.output}")
    
    # Compute and display statistics
    stats = compute_assembly_stats_from_records(contigs)
    print("\n" + stats.summary())
    
    # Save stats if requested
    if args.stats_file:
        with open(args.stats_file, "w") as f:
            f.write(stats.summary())
        print(f"\nStatistics saved to {args.stats_file}")


def cmd_simulate(args: argparse.Namespace) -> None:
    """Handle the simulate command."""
    from .io import read_fasta
    from .simulate import simulate_short_reads
    
    print(f"Reading reference from {args.reference}...")
    records = list(read_fasta(args.reference))
    
    if not records:
        print("Error: Reference file is empty", file=sys.stderr)
        sys.exit(1)
    
    reference = records[0].sequence
    print(f"Reference length: {len(reference):,} bp")
    
    print(f"Simulating {args.num_reads} reads...")
    reads = simulate_short_reads(
        reference,
        num_reads=args.num_reads // 2,
        read_length=args.read_length,
        error_rate=args.error_rate,
        seed=args.seed,
    )
    
    from .io import write_fastq
    write_fastq(reads, args.output)
    print(f"Wrote {len(reads)} reads to {args.output}")


def cmd_stats(args: argparse.Namespace) -> None:
    """Handle the stats command."""
    print(f"Reading contigs from {args.input}...")
    records = read_sequences(args.input)
    sequences = [r.sequence for r in records]
    
    stats = compute_assembly_stats(sequences)
    
    output = stats.summary()
    
    if args.output:
        with open(args.output, "w") as f:
            f.write(output)
        print(f"Statistics saved to {args.output}")
    else:
        print("\n" + output)


def main(argv: Optional[List[str]] = None) -> None:
    """Main entry point for the CLI."""
    parser = create_parser()
    args = parser.parse_args(argv)
    
    if args.command is None:
        parser.print_help()
        sys.exit(0)
    
    if args.command == "assemble":
        cmd_assemble(args)
    elif args.command == "simulate":
        cmd_simulate(args)
    elif args.command == "stats":
        cmd_stats(args)
    else:
        print(f"Unknown command: {args.command}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
