"""Round-trip tests for Huffman coding."""

import os
import pytest
from deflate_lite import huffman


def _roundtrip(data: bytes) -> bytes:
    payload, lengths = huffman.encode_bytes(data)
    return huffman.decode_bytes(payload, lengths, len(data))


def test_empty():
    assert _roundtrip(b"") == b""


def test_single_byte():
    assert _roundtrip(b"\x00") == b"\x00"
    assert _roundtrip(b"\xFF") == b"\xFF"


def test_all_same_byte():
    data = b"\x42" * 1000
    assert _roundtrip(data) == data


def test_two_unique_bytes():
    data = b"\x00\x01" * 500
    assert _roundtrip(data) == data


def test_text():
    data = b"hello world " * 100
    assert _roundtrip(data) == data


def test_random():
    data = os.urandom(1000)
    assert _roundtrip(data) == data


def test_all_256_bytes():
    data = bytes(range(256)) * 10
    assert _roundtrip(data) == data


def test_high_entropy():
    """Random data won't compress well but must round-trip exactly."""
    data = os.urandom(5000)
    assert _roundtrip(data) == data


def test_low_entropy():
    """Highly skewed data should compress well."""
    data = b"\x00" * 900 + b"\x01" * 100
    payload, lengths = huffman.encode_bytes(data)
    assert len(payload) < len(data), "Low-entropy data should compress"


def test_lengths_table_format():
    _, lengths = huffman.encode_bytes(b"hello")
    assert len(lengths) == 256
    assert all(0 <= l <= 15 for l in lengths)


def test_code_lengths_single_symbol():
    lengths = huffman.build_code_lengths([0] * 255 + [100])
    assert lengths[255] == 1  # single symbol gets length 1
    assert all(lengths[i] == 0 for i in range(255))


def test_canonical_codes_uniqueness():
    data = b"aaabbbcccdddeee" * 10
    _, lengths = huffman.encode_bytes(data)
    codes = huffman.canonical_codes_from_lengths(lengths)
    # All codes must be unique
    seen = set()
    for sym, (val, nbits) in codes.items():
        key = (val, nbits)
        assert key not in seen, f"Duplicate code for symbol {sym}"
        seen.add(key)


@pytest.mark.parametrize("size", [0, 1, 2, 10, 100, 1000, 5000])
def test_various_sizes(size):
    data = os.urandom(size)
    assert _roundtrip(data) == data


def test_writer_reader_lengths_roundtrip():
    """Test write_lengths / read_lengths round-trip."""
    from deflate_lite.bitio import BitWriter, BitReader

    lengths = list(range(16)) * 16  # 256 entries, 0..15 repeating
    writer = BitWriter()
    huffman.write_lengths(writer, lengths)
    data = writer.get_bytes()

    reader = BitReader(data)
    restored = huffman.read_lengths(reader)
    assert restored == lengths
