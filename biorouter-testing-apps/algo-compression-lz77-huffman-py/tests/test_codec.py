"""Round-trip tests for the full DEFLATE-lite codec (LZ77 → Huffman)."""

import os
import pytest
from deflate_lite import codec


# -------------------------------------------------------------------
# Core round-trip (v2 is the default path)
# -------------------------------------------------------------------

def _roundtrip(data: bytes, **kw) -> bytes:
    compressed = codec.compress_file(data, **kw)
    return codec.decompress_file(compressed)


# -------------------------------------------------------------------
# Edge cases
# -------------------------------------------------------------------

def test_empty():
    assert _roundtrip(b"") == b""


def test_single_byte():
    assert _roundtrip(b"\x00") == b"\x00"
    assert _roundtrip(b"\xFF") == b"\xFF"


def test_two_bytes():
    assert _roundtrip(b"\x00\x01") == b"\x00\x01"


# -------------------------------------------------------------------
# Text inputs
# -------------------------------------------------------------------

def test_short_text():
    data = b"hello world"
    assert _roundtrip(data) == data


def test_paragraph():
    data = (
        b"Compression is the process of reducing the size of data. "
        b"Lossless compression allows the original data to be perfectly "
        b"reconstructed from the compressed data."
    )
    assert _roundtrip(data) == data


def test_repetitive_text():
    data = b"abcdefghij" * 1000
    assert _roundtrip(data) == data


def test_long_english():
    text = (
        "The quick brown fox jumps over the lazy dog. "
        "Pack my box with five dozen liquor jugs. "
        "How vexingly quick daft zebras jump! "
        "Sphinx of black quartz, judge my vow. "
    ) * 200
    data = text.encode("utf-8")
    assert _roundtrip(data) == data


# -------------------------------------------------------------------
# Highly repetitive (best-case for LZ77)
# -------------------------------------------------------------------

def test_all_same():
    data = b"A" * 10_000
    result = _roundtrip(data)
    assert result == data
    # Should compress significantly
    compressed = codec.compress_file(data)
    assert len(compressed) < len(data)


def test_short_repeating_pattern():
    data = (b"ABC" * 3000)
    result = _roundtrip(data)
    assert result == data


# -------------------------------------------------------------------
# Binary / random (worst-case)
# -------------------------------------------------------------------

def test_random_1k():
    data = os.urandom(1024)
    assert _roundtrip(data) == data


def test_random_5k():
    data = os.urandom(5120)
    assert _roundtrip(data) == data


def test_random_10k():
    data = os.urandom(10_000)
    assert _roundtrip(data) == data


def test_binary_with_nulls():
    data = b"\x00" * 500 + os.urandom(200) + b"\x00" * 500
    assert _roundtrip(data) == data


def test_binary_all_zeroes():
    data = b"\x00" * 10_000
    assert _roundtrip(data) == data


def test_binary_all_ones():
    data = b"\xFF" * 10_000
    assert _roundtrip(data) == data


# -------------------------------------------------------------------
# Parametrised sweep
# -------------------------------------------------------------------

@pytest.mark.parametrize("size", [0, 1, 2, 3, 10, 50, 100, 500, 1000, 5000])
def test_parametrised_random(size):
    data = os.urandom(size)
    assert _roundtrip(data) == data


@pytest.mark.parametrize("size", [0, 1, 10, 100, 1000, 5000])
def test_parametrised_repetitive(size):
    data = b"XyZ" * max(1, size // 3 + 1)
    data = data[:size]
    assert _roundtrip(data) == data


# -------------------------------------------------------------------
# Window-size variants
# -------------------------------------------------------------------

def test_small_window():
    data = b"abcdefghij" * 200
    assert _roundtrip(data, window_size=64) == data


def test_large_window():
    data = b"hello" * 3000
    assert _roundtrip(data, window_size=8192) == data


# -------------------------------------------------------------------
# Container format sanity
# -------------------------------------------------------------------

def test_magic_present():
    data = b"test data"
    compressed = codec.compress_file(data)
    assert compressed[:4] == b"DLZ2"


def test_bad_magic_raises():
    with pytest.raises(ValueError, match="Bad magic"):
        codec.decompress_file(b"XXXX" + b"\x00" * 100)


# -------------------------------------------------------------------
# Compression effectiveness
# -------------------------------------------------------------------

def test_compresses_repetitive_data():
    data = b"ABCD" * 5000
    compressed = codec.compress_file(data)
    assert len(compressed) < len(data) * 0.5, "Highly repetitive data should compress well"


# -------------------------------------------------------------------
# Large round-trip (smoke)
# -------------------------------------------------------------------

def test_large_roundtrip():
    """Stress test: 100 KB of mixed content."""
    data = (
        os.urandom(10_000)
        + b"repeating text " * 2000
        + os.urandom(10_000)
        + b"\x00" * 5000
    )
    assert _roundtrip(data) == data
