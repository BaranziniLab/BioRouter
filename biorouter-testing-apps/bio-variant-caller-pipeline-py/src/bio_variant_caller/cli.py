"""Command-line interface for the variant-calling pipeline.

Runs the full pipeline: pileup → variant calling → annotation → VCF output.
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path
from typing import List, Optional

from .annotate import VariantAnnotator, ts_tv_ratio
from .caller import CallerConfig, VariantCaller
from .models import AlignedRead, Strand, Variant
from .pileup import PileupEngine
from .simulate import ReadSimulator, SimConfig, TruthVariant, simulate_reads
from .vcf import VCFWriter, write_vcf


# ---------------------------------------------------------------------------
# Pipeline runner
# ---------------------------------------------------------------------------

def run_pipeline(
    reference: str,
    reads: List[AlignedRead],
    config: Optional[CallerConfig] = None,
    ref_name: str = "ref",
    sample_name: str = "SAMPLE",
) -> tuple[List[Variant], dict]:
    """Run the full variant-calling pipeline.

    Returns (variants, stats_dict).
    """
    t0 = time.time()

    # Step 1: Pileup
    engine = PileupEngine(reference, reads)
    pileup = engine.build()
    t_pileup = time.time() - t0

    # Step 2: Call variants
    caller = VariantCaller(config=config, ref_name=ref_name)
    variants = caller.call(pileup)
    t_call = time.time() - t0 - t_pileup

    # Step 3: Annotate
    annotator = VariantAnnotator()
    variants = annotator.annotate(variants)
    t_annotate = time.time() - t0 - t_pileup - t_call

    t_total = time.time() - t0

    stats = {
        "reference_length": len(reference),
        "num_reads": len(reads),
        "covered_positions": len(pileup),
        "average_depth": (
            sum(pp.depth for pp in pileup.values()) / len(pileup)
            if pileup else 0.0
        ),
        "variants_called": len(variants),
        "snps": sum(1 for v in variants if v.variant_type.value == "SNP"),
        "indels": sum(1 for v in variants if v.variant_type.value in ("INS", "DEL")),
        "ts_tv_ratio": ts_tv_ratio(variants),
        "time_pileup_s": round(t_pileup, 4),
        "time_call_s": round(t_call, 4),
        "time_annotate_s": round(t_annotate, 4),
        "time_total_s": round(t_total, 4),
    }

    return variants, stats


# ---------------------------------------------------------------------------
# CLI argument parsing
# ---------------------------------------------------------------------------

def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="biovariantcall",
        description="Pure-Python variant-calling pipeline",
    )
    sub = parser.add_subparsers(dest="command")

    # --- run sub-command ---
    run_p = sub.add_parser("run", help="Run the full pipeline on input files")
    run_p.add_argument(
        "--reference", "-r", required=True,
        help="Path to a FASTA-like file containing the reference sequence",
    )
    run_p.add_argument(
        "--reads", "-R", required=True,
        help="Path to a TSV/JSON file containing aligned reads",
    )
    run_p.add_argument(
        "--output", "-o", default="output.vcf",
        help="Output VCF file path (default: output.vcf)",
    )
    run_p.add_argument(
        "--min-depth", type=int, default=8,
        help="Minimum depth to call a variant (default: 8)",
    )
    run_p.add_argument(
        "--min-af", type=float, default=0.2,
        help="Minimum allele frequency (default: 0.2)",
    )
    run_p.add_argument(
        "--min-base-quality", type=int, default=20,
        help="Minimum base quality (default: 20)",
    )
    run_p.add_argument(
        "--sample-name", default="SAMPLE",
        help="Sample name for VCF header (default: SAMPLE)",
    )
    run_p.add_argument(
        "--stats", "-s", default=None,
        help="Output stats JSON file path (optional)",
    )
    run_p.add_argument(
        "--json-input", action="store_true",
        help="Reads file is JSON format (default: tab-separated)",
    )

    # --- simulate sub-command ---
    sim_p = sub.add_parser("simulate", help="Simulate reads with injected variants")
    sim_p.add_argument(
        "--reference", "-r", required=True,
        help="Path to reference sequence file",
    )
    sim_p.add_argument(
        "--output-reads", "-o", default="simulated_reads.tsv",
        help="Output reads file (TSV format, default: simulated_reads.tsv)",
    )
    sim_p.add_argument(
        "--output-truth", "-t", default="truth_variants.tsv",
        help="Output truth variants file (default: truth_variants.tsv)",
    )
    sim_p.add_argument(
        "--coverage", "-c", type=float, default=30.0,
        help="Average coverage depth (default: 30)",
    )
    sim_p.add_argument(
        "--read-length", type=int, default=150,
        help="Read length in bp (default: 150)",
    )
    sim_p.add_argument(
        "--error-rate", type=float, default=0.01,
        help="Per-base error rate (default: 0.01)",
    )
    sim_p.add_argument(
        "--seed", type=int, default=42,
        help="Random seed (default: 42)",
    )
    sim_p.add_argument(
        "--variants", nargs="*", default=[],
        help="Variant positions to inject (space-separated POS:REF:ALT, e.g. 10:A:G)",
    )

    # --- eval sub-command ---
    eval_p = sub.add_parser("eval", help="Evaluate a VCF against truth variants")
    eval_p.add_argument(
        "--vcf", "-v", required=True,
        help="Called VCF file",
    )
    eval_p.add_argument(
        "--truth", "-t", required=True,
        help="Truth variants file (TSV: chrom pos ref alt)",
    )
    eval_p.add_argument(
        "--tolerance", type=int, default=0,
        help="Position tolerance for matching (default: 0 exact)",
    )

    return parser


# ---------------------------------------------------------------------------
# File I/O helpers
# ---------------------------------------------------------------------------

def load_reference(filepath: str) -> str:
    """Load a reference sequence from a file (plain text or minimal FASTA)."""
    with open(filepath) as fh:
        lines = fh.read().splitlines()
    # Skip FASTA headers
    seq_lines = []
    for line in lines:
        if line.startswith(">"):
            continue
        seq_lines.append(line.strip())
    return "".join(seq_lines).upper()


def load_reads_tsv(filepath: str) -> List[AlignedRead]:
    """Load reads from a tab-separated file.

    Format: name  ref_start  cigar  sequence  qualities  strand  mapq
    """
    reads: List[AlignedRead] = []
    with open(filepath) as fh:
        for line in fh:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) < 5:
                continue
            name = parts[0]
            ref_start = int(parts[1])
            cigar = parts[2]
            sequence = parts[3]
            quals = [int(q) for q in parts[4].split(",")]
            strand = Strand.FORWARD if len(parts) < 6 or parts[5] in ("+", "F", "0") else Strand.REVERSE
            mapq = int(parts[6]) if len(parts) > 6 else 60
            reads.append(AlignedRead(
                name=name,
                ref_start=ref_start,
                cigar=cigar,
                sequence=sequence,
                base_qualities=quals,
                strand=strand,
                map_quality=mapq,
            ))
    return reads


def load_reads_json(filepath: str) -> List[AlignedRead]:
    """Load reads from a JSON file."""
    with open(filepath) as fh:
        data = json.load(fh)
    reads: List[AlignedRead] = []
    for r in data:
        strand_str = r.get("strand", "+")
        strand = Strand.FORWARD if strand_str in ("+", "F", "forward", "0") else Strand.REVERSE
        reads.append(AlignedRead(
            name=r["name"],
            ref_start=r["ref_start"],
            cigar=r["cigar"],
            sequence=r["sequence"],
            base_qualities=r["base_qualities"],
            strand=strand,
            map_quality=r.get("map_quality", 60),
        ))
    return reads


def save_reads_tsv(reads: List[AlignedRead], filepath: str) -> None:
    """Save reads to a TSV file."""
    with open(filepath, "w") as fh:
        for r in reads:
            strand = "+" if r.strand == Strand.FORWARD else "-"
            quals = ",".join(str(q) for q in r.base_qualities)
            fh.write(
                f"{r.name}\t{r.ref_start}\t{r.cigar}\t{r.sequence}"
                f"\t{quals}\t{strand}\t{r.map_quality}\n"
            )


def save_truth_tsv(truth: List[TruthVariant], filepath: str) -> None:
    """Save truth variants to a TSV file."""
    with open(filepath, "w") as fh:
        fh.write("#chrom\tpos\tref\talt\ttype\n")
        for tv in truth:
            fh.write(
                f"sim\t{tv.pos}\t{tv.ref}\t{tv.alt}\t{tv.variant_type.value}\n"
            )


def load_truth_tsv(filepath: str) -> List[TruthVariant]:
    """Load truth variants from a TSV file."""
    from .models import VariantType
    truth: List[TruthVariant] = []
    with open(filepath) as fh:
        for line in fh:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) < 4:
                continue
            vtype_str = parts[4] if len(parts) > 4 else "SNP"
            try:
                vtype = VariantType(vtype_str)
            except ValueError:
                vtype = VariantType.SNP
            truth.append(TruthVariant(
                pos=int(parts[1]),
                ref=parts[2],
                alt=parts[3],
                variant_type=vtype,
            ))
    return truth


# ---------------------------------------------------------------------------
# Sub-command handlers
# ---------------------------------------------------------------------------

def cmd_run(args: argparse.Namespace) -> int:
    """Execute the 'run' sub-command."""
    reference = load_reference(args.reference)

    if args.json_input:
        reads = load_reads_json(args.reads)
    else:
        reads = load_reads_tsv(args.reads)

    config = CallerConfig(
        min_depth=args.min_depth,
        min_alt_allele_frequency=args.min_af,
        min_base_quality=args.min_base_quality,
    )

    variants, stats = run_pipeline(
        reference, reads, config=config,
        ref_name="sim", sample_name=args.sample_name,
    )

    write_vcf(variants, args.output, sample_name=args.sample_name, reference_name="sim")
    print(f"Wrote {len(variants)} variants to {args.output}")

    # Print stats
    print(f"\n--- Pipeline Statistics ---")
    print(f"  Reference length:  {stats['reference_length']:,} bp")
    print(f"  Number of reads:   {stats['num_reads']:,}")
    print(f"  Covered positions: {stats['covered_positions']:,}")
    print(f"  Average depth:     {stats['average_depth']:.1f}x")
    print(f"  Variants called:   {stats['variants_called']}")
    print(f"    SNPs:            {stats['snps']}")
    print(f"    Indels:          {stats['indels']}")
    print(f"    Ts/Tv ratio:     {stats['ts_tv_ratio']:.2f}")
    print(f"  Time (pileup):     {stats['time_pileup_s']:.3f}s")
    print(f"  Time (calling):    {stats['time_call_s']:.3f}s")
    print(f"  Time (annotate):   {stats['time_annotate_s']:.3f}s")
    print(f"  Time (total):      {stats['time_total_s']:.3f}s")

    if args.stats:
        with open(args.stats, "w") as fh:
            json.dump(stats, fh, indent=2)
        print(f"\nStats written to {args.stats}")

    return 0


def cmd_simulate(args: argparse.Namespace) -> int:
    """Execute the 'simulate' sub-command."""
    reference = load_reference(args.reference)

    sim_config = SimConfig(
        seed=args.seed,
        read_length=args.read_length,
        coverage=args.coverage,
        error_rate=args.error_rate,
    )

    sim = ReadSimulator(reference, sim_config)

    # Parse variant specifications
    for vstr in args.variants:
        parts = vstr.split(":")
        if len(parts) < 2:
            print(f"Warning: skipping invalid variant spec '{vstr}' (expected POS:REF:ALT)")
            continue
        pos = int(parts[0])
        ref = parts[1] if len(parts) > 1 else reference[pos]
        alt = parts[2] if len(parts) > 2 else None
        sim.add_variant(pos, ref=ref, alt=alt)

    reads, truth = sim.simulate()

    save_reads_tsv(reads, args.output_reads)
    save_truth_tsv(truth, args.output_truth)

    print(f"Simulated {len(reads)} reads from {len(reference):,} bp reference")
    print(f"  Coverage: ~{args.coverage:.1f}x")
    print(f"  Injected {len(truth)} variant(s)")
    print(f"  Reads written to: {args.output_reads}")
    print(f"  Truth written to: {args.output_truth}")

    return 0


def cmd_eval(args: argparse.Namespace) -> int:
    """Execute the 'eval' sub-command."""
    from .caller import CallerConfig

    # Load truth
    truth = load_truth_tsv(args.truth)

    # Load called variants from VCF (simplified parser)
    called: List[Variant] = []
    with open(args.vcf) as fh:
        for line in fh:
            if line.startswith("#"):
                continue
            parts = line.strip().split("\t")
            if len(parts) < 8:
                continue
            v = Variant(
                chrom=parts[0],
                pos=int(parts[1]) - 1,  # VCF is 1-based
                ref=parts[3],
                alt=parts[4],
                variant_type=_guess_type_simple(parts[3], parts[4]),
            )
            called.append(v)

    # Evaluate
    tol = args.tolerance
    tp = 0
    truth_matched = set()

    for c in called:
        for i, t in enumerate(truth):
            if i in truth_matched:
                continue
            if (
                c.pos + tol >= t.pos and c.pos - tol <= t.pos
                and c.ref == t.ref
                and c.alt == t.alt
            ):
                tp += 1
                truth_matched.add(i)
                c.is_true_positive = True
                break

    fp = len(called) - tp
    fn = len(truth) - tp
    precision = tp / (tp + fp) if (tp + fp) > 0 else 0.0
    sensitivity = tp / (tp + fn) if (tp + fn) > 0 else 0.0
    f1 = 2 * precision * sensitivity / (precision + sensitivity) if (precision + sensitivity) > 0 else 0.0

    print(f"--- Evaluation Results ---")
    print(f"  Truth variants:   {len(truth)}")
    print(f"  Called variants:  {len(called)}")
    print(f"  True positives:   {tp}")
    print(f"  False positives:  {fp}")
    print(f"  False negatives:  {fn}")
    print(f"  Precision:        {precision:.3f}")
    print(f"  Sensitivity:      {sensitivity:.3f}")
    print(f"  F1 score:         {f1:.3f}")

    return 0 if fn == 0 and fp == 0 else (1 if sensitivity < 0.5 else 0)


def _guess_type_simple(ref: str, alt: str) -> "VariantType":
    from .models import VariantType
    if len(ref) == 1 and len(alt) == 1:
        return VariantType.SNP
    elif len(ref) < len(alt):
        return VariantType.INSERTION
    elif len(ref) > len(alt):
        return VariantType.DELETION
    return VariantType.MNP


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def main(argv: Optional[List[str]] = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    if args.command == "run":
        return cmd_run(args)
    elif args.command == "simulate":
        return cmd_simulate(args)
    elif args.command == "eval":
        return cmd_eval(args)
    else:
        parser.print_help()
        return 1


if __name__ == "__main__":
    sys.exit(main())
