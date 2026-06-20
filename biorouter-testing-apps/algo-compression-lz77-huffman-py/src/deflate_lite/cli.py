"""
Command-line interface for deflate-lite.

Usage
-----
    deflate-lite compress  <input> <output>   [--window N] [--lookahead N]
    deflate-lite decompress <input> <output>
    deflate-lite analyze   <input> <compressed>
    deflate-lite info      <input>

All operations print timing information to stderr.
"""

from __future__ import annotations

import argparse
import os
import sys
import time
from pathlib import Path

from deflate_lite import codec, analyze


def _read(path: str) -> bytes:
    with open(path, "rb") as f:
        return f.read()


def _write(path: str, data: bytes) -> None:
    with open(path, "wb") as f:
        f.write(data)


def cmd_compress(args: argparse.Namespace) -> None:
    data = _read(args.input)
    t0 = time.perf_counter()
    compressed = codec.compress_file(data, window_size=args.window, lookahead_size=args.lookahead)
    elapsed = time.perf_counter() - t0

    _write(args.output, compressed)

    a = analyze.analyze(data, compressed)
    print(analyze.format_report(a))
    print(f"Time            : {elapsed:.3f}s")
    if elapsed > 0:
        throughput = len(data) / elapsed / 1_048_576
        print(f"Throughput      : {throughput:.2f} MB/s")


def cmd_decompress(args: argparse.Namespace) -> None:
    data = _read(args.input)
    t0 = time.perf_counter()
    decompressed = codec.decompress_file(data)
    elapsed = time.perf_counter() - t0

    _write(args.output, decompressed)
    print(f"Decompressed {len(decompressed):,} bytes in {elapsed:.3f}s")


def cmd_analyze(args: argparse.Namespace) -> None:
    original = _read(args.input)
    compressed = _read(args.compressed)
    a = analyze.analyze(original, compressed)
    print(analyze.format_report(a))
    ent = analyze.shannon_entropy(original)
    print(f"Shannon entropy : {ent:.4f} bits/byte")


def cmd_info(args: argparse.Namespace) -> None:
    data = _read(args.input)
    ent = analyze.shannon_entropy(data)
    print(f"File            : {args.input}")
    print(f"Size            : {len(data):,} bytes")
    print(f"Shannon entropy : {ent:.4f} bits/byte")
    print(f"Theoretical min : {ent * len(data) / 8:,.0f} bytes")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="deflate-lite",
        description="LZ77 + Huffman (DEFLATE-lite) compression toolkit",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    # compress
    p_comp = sub.add_parser("compress", help="Compress a file")
    p_comp.add_argument("input", help="Input file path")
    p_comp.add_argument("output", help="Output file path")
    p_comp.add_argument("--window", type=int, default=4096, help="LZ77 window size (default 4096)")
    p_comp.add_argument("--lookahead", type=int, default=258, help="LZ77 lookahead size (default 258)")

    # decompress
    p_decomp = sub.add_parser("decompress", help="Decompress a file")
    p_decomp.add_argument("input", help="Compressed file path")
    p_decomp.add_argument("output", help="Output file path")

    # analyze
    p_anal = sub.add_parser("analyze", help="Analyze compression ratio")
    p_anal.add_argument("input", help="Original file path")
    p_anal.add_argument("compressed", help="Compressed file path")

    # info
    p_info = sub.add_parser("info", help="Show file entropy info")
    p_info.add_argument("input", help="File path")

    return parser


def main(argv: list[str] | None = None) -> None:
    parser = build_parser()
    args = parser.parse_args(argv)

    dispatch = {
        "compress": cmd_compress,
        "decompress": cmd_decompress,
        "analyze": cmd_analyze,
        "info": cmd_info,
    }
    dispatch[args.command](args)


if __name__ == "__main__":
    main()
