"""Tests for the entropy / compression analysis module."""

import os
from deflate_lite import analyze


def test_entropy_uniform():
    """Uniform data has maximum entropy (~8 bits/byte)."""
    data = os.urandom(10_000)
    ent = analyze.shannon_entropy(data)
    assert 7.5 < ent <= 8.0


def test_entropy_zero():
    """All-same data has zero entropy."""
    data = b"\x00" * 1000
    ent = analyze.shannon_entropy(data)
    assert ent == 0.0


def test_entropy_empty():
    assert analyze.shannon_entropy(b"") == 0.0


def test_analyze_basic():
    original = b"hello" * 100
    compressed = b"\x00" * 10  # fake small compressed
    a = analyze.analyze(original, compressed)
    assert a.original_size == 500
    assert a.compressed_size == 10
    assert a.ratio == 10 / 500
    assert a.space_saving > 0.9


def test_analyze_empty():
    a = analyze.analyze(b"", b"")
    assert a.original_size == 0
    assert a.compressed_size == 0


def test_format_report():
    a = analyze.Analysis(
        original_size=1000,
        compressed_size=500,
        ratio=0.5,
        space_saving=0.5,
        shannon_entropy=4.0,
        bits_per_byte=4.0,
    )
    report = analyze.format_report(a)
    assert "1,000" in report
    assert "50.0%" in report
