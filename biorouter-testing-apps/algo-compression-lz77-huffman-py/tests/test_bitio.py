"""Round-trip tests for BitWriter / BitReader."""

import os
from deflate_lite.bitio import BitWriter, BitReader


def test_single_bit_roundtrip():
    writer = BitWriter()
    writer.write_bit(1)
    writer.write_bit(0)
    writer.write_bit(1)
    data = writer.get_bytes()

    reader = BitReader(data)
    assert reader.read_bit() == 1
    assert reader.read_bit() == 0
    assert reader.read_bit() == 1


def test_write_bits_roundtrip():
    writer = BitWriter()
    writer.write_bits(0b10110, 5)   # 5 bits
    writer.write_bits(0b110011, 6)  # 6 bits
    writer.write_bits(0xFF, 8)      # 8 bits
    data = writer.get_bytes()

    reader = BitReader(data)
    assert reader.read_bits(5) == 0b10110
    assert reader.read_bits(6) == 0b110011
    assert reader.read_bits(8) == 0xFF


def test_byte_boundary_roundtrip():
    writer = BitWriter()
    writer.write_bits(0xABCD, 16)
    data = writer.get_bytes()
    assert data == b"\xAB\xCD" or data == b"\xCD\xAB"  # LSB-first: CD then AB
    reader = BitReader(data)
    assert reader.read_bits(16) == 0xABCD


def test_write_bytes():
    writer = BitWriter()
    writer.write_bit(1)
    writer.write_bytes(b"hello")
    data = writer.get_bytes()
    reader = BitReader(data)
    assert reader.read_bit() == 1
    # Should be aligned after flush before write_bytes
    assert reader.read_bytes(5) == b"hello"


def test_len_methods():
    writer = BitWriter()
    assert len(writer) == 0
    writer.write_bits(0xFF, 3)
    assert len(writer) == 3
    writer.write_bit(1)
    assert len(writer) == 4

    reader = BitReader(b"\xFF\xFF")
    assert len(reader) == 16
    reader.read_bits(5)
    assert len(reader) == 11


def test_remaining_bits():
    data = b"\xAB\xCD\xEF"
    reader = BitReader(data)
    assert reader.remaining_bits() == 24
    reader.read_bits(10)
    assert reader.remaining_bits() == 14


def test_aligned():
    writer = BitWriter()
    writer.write_bits(0xFF, 8)
    data = writer.get_bytes()
    reader = BitReader(data)
    assert reader.aligned()
    reader.read_bit()
    assert not reader.aligned()


def test_eof_error():
    reader = BitReader(b"\x01")
    reader.read_bit()
    import pytest
    with pytest.raises(EOFError):
        reader.read_bits(10)


def test_random_bytes_roundtrip():
    """Write 1000 random bytes and read them back."""
    original = os.urandom(1000)
    writer = BitWriter()
    for b in original:
        writer.write_bits(b, 8)
    data = writer.get_bytes()

    reader = BitReader(data)
    result = bytearray()
    for _ in range(1000):
        result.append(reader.read_bits(8))
    assert bytes(result) == original
