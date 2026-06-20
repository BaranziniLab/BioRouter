"""
Entropy and compression-ratio analysis.

Provides tools to measure:
- Shannon entropy of input data.
- Compression ratio (compressed / original).
- Bits-per-byte statistics.
"""

from __future__ import annotations

import math
from collections import Counter
from dataclasses import dataclass
from typing import Dict


@dataclass(frozen=True, slots=True)
class Analysis:
    """Results of an entropy / compression analysis."""
    original_size: int
    compressed_size: int
    ratio: float            # compressed / original (< 1 means compression)
    space_saving: float     # 1 - ratio (positive = saving)
    shannon_entropy: float  # bits per byte of original
    bits_per_byte: float    # (compressed_bits / original_bytes); 8 = no compression


def shannon_entropy(data: bytes) -> float:
    """
    Compute Shannon entropy of *data* in bits per byte.

    Returns a value between 0.0 (all identical bytes) and 8.0
    (uniformly random).
    """
    if not data:
        return 0.0

    n = len(data)
    freqs = Counter(data)
    entropy = 0.0
    for count in freqs.values():
        p = count / n
        entropy -= p * math.log2(p)
    return entropy


def analyze(original: bytes, compressed: bytes) -> Analysis:
    """
    Compare original and compressed byte strings.

    Returns an Analysis dataclass with ratio, entropy, and
    bits-per-byte metrics.
    """
    orig_size = len(original)
    comp_size = len(compressed)

    if orig_size == 0:
        return Analysis(0, comp_size, 0.0, 1.0, 0.0, 0.0)

    ratio = comp_size / orig_size
    saving = 1.0 - ratio
    entropy = shannon_entropy(original)
    bpb = (comp_size * 8) / orig_size

    return Analysis(
        original_size=orig_size,
        compressed_size=comp_size,
        ratio=ratio,
        space_saving=saving,
        shannon_entropy=entropy,
        bits_per_byte=bpb,
    )


def format_report(a: Analysis) -> str:
    """Pretty-print an Analysis as a multi-line report."""
    lines = [
        f"Original size   : {a.original_size:,} bytes",
        f"Compressed size : {a.compressed_size:,} bytes",
        f"Ratio           : {a.ratio:.4f}  ({a.space_saving * 100:.1f}% saving)",
        f"Bits per byte   : {a.bits_per_byte:.2f}  (entropy ≈ {a.shannon_entropy:.2f} bits/byte)",
    ]
    return "\n".join(lines)
