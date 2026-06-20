"""Command-line interface for strmatch.

Usage:
    strmatch search <pattern> <file> [--algo NAME] [--time] [--count]
    strmatch search --patterns <patfile> <file> [--algo NAME] [--time] [--count]
    strmatch compare <pattern> <file> [--algos NAME,...] [--repeats N]
"""

from __future__ import annotations

import argparse
import sys
import time

from strmatch.bench import EXACT_ALGORITHMS, get_algorithm, benchmark_all
from strmatch.multi import aho_corasick_search
from strmatch.approx import k_mismatch_search


def _read_file(path: str) -> str:
    with open(path, encoding="utf-8") as f:
        return f.read()


def _cmd_search(args: argparse.Namespace) -> None:
    text = _read_file(args.file)

    # Multi-pattern mode (--patterns file or aho-corasick algo)
    if args.patterns_file:
        with open(args.patterns_file, encoding="utf-8") as f:
            patterns = [line.rstrip("\n") for line in f if line.strip()]
        start = time.perf_counter()
        results = aho_corasick_search(text, patterns)
        elapsed = time.perf_counter() - start
        for pos, pat in results:
            print(f"{pos}\t{pat}")
        if args.time:
            print(f"\nTime: {elapsed:.6f}s ({len(results)} matches)")
        if args.count:
            print(f"Count: {len(results)}")
        return

    pattern = args.pattern
    algo_name = args.algo or "kmp"

    if algo_name == "aho-corasick":
        start = time.perf_counter()
        results = aho_corasick_search(text, [pattern])
        elapsed = time.perf_counter() - start
        positions = [r[0] for r in results]
    else:
        algo = get_algorithm(algo_name)
        start = time.perf_counter()
        positions = algo(text, pattern)
        elapsed = time.perf_counter() - start

    for pos in positions:
        print(pos)

    if args.time:
        print(f"\nTime: {elapsed:.6f}s ({len(positions)} matches)")
    if args.count:
        print(f"Count: {len(positions)}")


def _cmd_compare(args: argparse.Namespace) -> None:
    text = _read_file(args.file)
    pattern = args.pattern
    algos = args.algos.split(",") if args.algos else None
    repeats = args.repeats

    results = benchmark_all(text, pattern, algorithms=algos, repeats=repeats)

    # Header
    print(f"{'Algorithm':<16} {'Matches':>8} {'Time (s)':>12}")
    print("-" * 40)
    for name, (count, elapsed) in sorted(results.items(), key=lambda x: x[1][1]):
        print(f"{name:<16} {count:>8} {elapsed:>12.6f}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="strmatch",
        description="String-matching and text-indexing CLI.",
    )
    sub = parser.add_subparsers(dest="command")

    # --- search ---
    sp = sub.add_parser("search", help="Search for a pattern in a text file.")
    sp.add_argument("pattern", nargs="?", default=None, help="Pattern string to search for.")
    sp.add_argument("file", help="Text file to search in.")
    sp.add_argument("--algo", default="kmp", choices=list(EXACT_ALGORITHMS) + ["aho-corasick"],
                     help="Algorithm to use (default: kmp).")
    sp.add_argument("--patterns", dest="patterns_file", default=None,
                     help="File with one pattern per line (activates Aho-Corasick).")
    sp.add_argument("--time", action="store_true", help="Show elapsed time.")
    sp.add_argument("--count", action="store_true", help="Show match count.")
    sp.add_argument("-k", "--mismatch", type=int, default=None,
                     help="Allow up to k mismatches (Hamming distance).")

    # --- compare ---
    cp = sub.add_parser("compare", help="Benchmark algorithms on the same input.")
    cp.add_argument("pattern", help="Pattern string.")
    cp.add_argument("file", help="Text file.")
    cp.add_argument("--algos", default=None,
                     help="Comma-separated algorithm names (default: all).")
    cp.add_argument("--repeats", type=int, default=3, help="Runs to average (default: 3).")

    return parser


def main(argv: list[str] | None = None) -> None:
    parser = build_parser()
    args = parser.parse_args(argv)

    if args.command == "search":
        _cmd_search(args)
    elif args.command == "compare":
        _cmd_compare(args)
    else:
        parser.print_help()
        sys.exit(1)


if __name__ == "__main__":
    main()
