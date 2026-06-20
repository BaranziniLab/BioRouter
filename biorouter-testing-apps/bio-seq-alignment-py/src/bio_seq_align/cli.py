"""Command-line interface for bio-seq-align."""

from __future__ import annotations

import argparse
import sys

from .align.nw import needleman_wunsch
from .align.sw import smith_waterman
from .align.gotoh import gotoh_align
from .align.banded import banded_alignment
from .align.semi_global import semi_global_alignment, overlap_alignment
from .matrices import get_matrix
from .fasta import read_fasta
from .msa import progressive_msa

# ── ANSI colors ──────────────────────────────────────────────

RESET  = "\033[0m"
GREEN  = "\033[32m"
RED    = "\033[31m"
YELLOW = "\033[33m"
CYAN   = "\033[36m"
BOLD   = "\033[1m"


def _color_alignment(aligned1: str, aligned2: str, width: int = 60) -> str:
    """Return a colored alignment string."""
    lines: list[str] = []
    for start in range(0, len(aligned1), width):
        s1 = aligned1[start : start + width]
        s2 = aligned2[start : start + width]

        mid_chars: list[str] = []
        for a, b in zip(s1, s2):
            if a == b:
                mid_chars.append(f"{GREEN}|{RESET}")
            elif a == "-" or b == "-":
                mid_chars.append(f"{RED} {RESET}")
            else:
                mid_chars.append(f"{YELLOW}.{RESET}")
        mid = "".join(mid_chars)

        # Colorize sequences
        c1_parts: list[str] = []
        c2_parts: list[str] = []
        for a, b in zip(s1, s2):
            if a == b and a != "-":
                c1_parts.append(f"{GREEN}{a}{RESET}")
                c2_parts.append(f"{GREEN}{b}{RESET}")
            elif a == "-" or b == "-":
                c1_parts.append(f"{RED}{a}{RESET}")
                c2_parts.append(f"{RED}{b}{RESET}")
            else:
                c1_parts.append(f"{YELLOW}{a}{RESET}")
                c2_parts.append(f"{YELLOW}{b}{RESET}")

        pos = start + 1
        lines.append(f"  Seq1 {pos:>5}  {''.join(c1_parts)}")
        lines.append(f"                {mid}")
        lines.append(f"  Seq2 {pos:>5}  {''.join(c2_parts)}")
        lines.append("")

    return "\n".join(lines)


# ── Algorithm dispatch ───────────────────────────────────────

ALGORITHMS = {
    "nw": ("Needleman-Wunsch", needleman_wunsch),
    "sw": ("Smith-Waterman", smith_waterman),
    "gotoh": ("Gotoh", gotoh_align),
    "banded": ("Banded-NW", banded_alignment),
    "semi-global": ("Semi-global", semi_global_alignment),
    "overlap": ("Overlap", overlap_alignment),
}


def main(argv: list[str] | None = None) -> None:
    """Entry point for the bio-seq-align CLI."""
    parser = argparse.ArgumentParser(
        prog="bio-seq-align",
        description="Biological sequence alignment toolkit.",
    )
    parser.add_argument("--seq1", help="First sequence (protein or DNA)")
    parser.add_argument("--seq2", help="Second sequence (protein or DNA)")
    parser.add_argument("--fasta", help="FASTA file (uses first two sequences)")
    parser.add_argument(
        "--algo", choices=list(ALGORITHMS.keys()) + ["msa"],
        default="nw", help="Alignment algorithm (default: nw)",
    )
    parser.add_argument("--matrix", default=None, help="Substitution matrix (blosum62, simple, dna)")
    parser.add_argument("--gap", type=int, default=-2, help="Linear gap penalty (default: -2)")
    parser.add_argument("--gap-open", type=int, default=-5, help="Affine gap open penalty (for gotoh)")
    parser.add_argument("--gap-extend", type=int, default=-1, help="Affine gap extend penalty (for gotoh)")
    parser.add_argument("--match", type=int, default=2, help="Match score for simple matrix")
    parser.add_argument("--mismatch", type=int, default=-1, help="Mismatch score for simple matrix")
    parser.add_argument("--bandwidth", type=int, default=3, help="Half-bandwidth for banded alignment")
    parser.add_argument("--no-color", action="store_true", help="Disable colored output")
    parser.add_argument("--block", type=int, default=60, help="Alignment block width")

    args = parser.parse_args(argv)

    # Resolve sequences
    seq1 = args.seq1
    seq2 = args.seq2

    if args.fasta:
        records = read_fasta(args.fasta)
        if len(records) < 2:
            print("Error: FASTA file must contain at least 2 sequences.", file=sys.stderr)
            sys.exit(1)
        seq1 = records[0].sequence
        seq2 = records[1].sequence
        print(f"Loaded {len(records)} sequences from {args.fasta}")
        print(f"  {records[0].id}: {len(records[0])} residues")
        print(f"  {records[1].id}: {len(records[1])} residues")
        print()

    if seq1 is None or seq2 is None:
        # Interactive prompt
        if seq1 is None:
            seq1 = input("Enter sequence 1: ").strip()
        if seq2 is None:
            seq2 = input("Enter sequence 2: ").strip()

    if not seq1 or not seq2:
        print("Error: both sequences must be non-empty.", file=sys.stderr)
        sys.exit(1)

    # Run alignment
    if args.algo == "msa":
        # Multiple sequence alignment mode
        if args.fasta:
            records = read_fasta(args.fasta)
            sequences = [r.sequence for r in records]
            labels = [r.id for r in records]
        else:
            sequences = [seq1, seq2]
            labels = ["Seq1", "Seq2"]

        aligned = progressive_msa(
            sequences, labels,
            matrix=args.matrix or "simple",
            gap_penalty=args.gap,
            match=args.match,
            mismatch=args.mismatch,
        )

        print(f"{BOLD}Progressive MSA Results{RESET}")
        print("=" * 60)
        for label, seq in zip(labels, aligned):
            print(f"  {CYAN}{label:<10}{RESET} {seq}")
        print()
        print(f"  Aligned length: {len(aligned[0])}")
    else:
        name, func = ALGORITHMS[args.algo]

        kwargs: dict = {}
        if args.matrix:
            kwargs["matrix"] = args.matrix
        if args.algo == "gotoh":
            kwargs["gap_open"] = args.gap_open
            kwargs["gap_extend"] = args.gap_extend
        elif args.algo == "banded":
            kwargs["bandwidth"] = args.bandwidth
        else:
            kwargs["gap_penalty"] = args.gap

        result = func(seq1, seq2, **kwargs)

        print(f"{BOLD}{name} Alignment{RESET}")
        print("=" * 60)
        print()
        print(result.summary())
        print()
        print(f"{BOLD}Alignment:{RESET}")
        if args.no_color:
            for line in result.alignment_lines(args.block):
                print(f"  {line}")
        else:
            print(_color_alignment(result.aligned_seq1, result.aligned_seq2, args.block))


if __name__ == "__main__":
    main()
