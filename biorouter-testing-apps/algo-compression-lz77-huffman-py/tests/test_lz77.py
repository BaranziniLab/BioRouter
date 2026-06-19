"""Round-trip tests for LZ77 encoder / decoder."""

import os
import pytest
from deflate_lite import lz77


# -------------------------------------------------------------------
# Round-trip helpers
# -------------------------------------------------------------------

def _roundtrip_tokens(data: bytes, **kw) -> bytes:
    tokens = lz77.encode(data, **kw)
    return lz77.decode(tokens)


def _roundtrip_bytes(data: bytes, **kw) -> bytes:
    encoded = lz77.encode_to_bytes(data, **kw)
    return lz77.decode_from_bytes(encoded)


# -------------------------------------------------------------------
# Basic tests
# -------------------------------------------------------------------

def test_empty():
    assert _roundtrip_tokens(b"") == b""
    assert _roundtrip_bytes(b"") == b""


def test_single_byte():
    assert _roundtrip_tokens(b"\x42") == b"\x42"
    assert _roundtrip_bytes(b"\x42") == b"\x42"


def test_literal_only():
    """All unique bytes — no back-reference possible."""
    data = bytes(range(256))
    assert _roundtrip_tokens(data) == data
    assert _roundtrip_bytes(data) == data


def test_repetitive_text():
    data = b"hello hello hello hello hello world " * 50
    assert _roundtrip_tokens(data) == data
    assert _roundtrip_bytes(data) == data


def test_highly_repetitive():
    data = b"A" * 10_000
    assert _roundtrip_tokens(data) == data
    assert _roundtrip_bytes(data) == data


def test_random_bytes():
    data = os.urandom(2000)
    assert _roundtrip_tokens(data) == data
    assert _roundtrip_bytes(data) == data


def test_binary_with_nulls():
    data = b"\x00" * 100 + b"\xFF" * 100 + b"\x00" * 100
    assert _roundtrip_tokens(data) == data
    assert _roundtrip_bytes(data) == data


def test_mixed_patterns():
    data = (b"abc" * 200) + (b"xyz" * 200) + (b"abc" * 200)
    assert _roundtrip_tokens(data) == data
    assert _roundtrip_bytes(data) == data


def test_longer_text():
    text = (
        "The quick brown fox jumps over the lazy dog. "
        "Pack my box with five dozen liquor jugs. "
        "How vexingly quick daft zebras jump! "
    ) * 100
    data = text.encode("utf-8")
    assert _roundtrip_tokens(data) == data
    assert _roundtrip_bytes(data) == data


def test_custom_window_sizes():
    data = b"abcdef" * 50
    for ws in [64, 256, 1024, 4096]:
        assert _roundtrip_tokens(data, window_size=ws) == data


def test_compresses_repetitive():
    """Repetitive data should actually compress (fewer tokens than bytes)."""
    data = b"ABCABC" * 500  # 3000 bytes
    tokens = lz77.encode(data)
    assert len(tokens) < len(data)


@pytest.mark.parametrize("size", [0, 1, 2, 3, 10, 100, 1000, 5000])
def test_various_sizes(size):
    data = os.urandom(size)
    assert _roundtrip_bytes(data) == data


def test_10k_random():
    data = os.urandom(10_000)
    assert _roundtrip_bytes(data) == data
