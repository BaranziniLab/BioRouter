"""
DEFLATE-lite codec — combined LZ77 -> Huffman pipeline.

File container format (DLZ2, the default)
------------------------------------------
    Magic bytes:         b'DLZ2'   (4 bytes)
    Flags:               1 byte    (currently 0; reserved)
    Original length:     8 bytes, little-endian uint64
    LZ stream length:    8 bytes, little-endian uint64
    Code-length table:   256 x 4 bits = 128 bytes (canonical Huffman header)
    Compressed payload:  variable length (Huffman-coded LZ77 token stream)

The LZ77 token serialisation (inside the Huffman-coded payload) uses
the format defined in lz77.encode_to_bytes.
"""

from __future__ import annotations

import struct
from typing import Tuple

from deflate_lite import lz77, huffman
from deflate_lite.bitio import BitReader, BitWriter

MAGIC_V1 = b"DLZ1"
MAGIC_V2 = b"DLZ2"
HEADER_FLAGS = 0


# -------------------------------------------------------------------
# Compress (v2 — stores LZ stream length for exact decoding)
# -------------------------------------------------------------------

def compress(data: bytes, window_size: int = 4096, lookahead_size: int = 258) -> bytes:
    """
    Compress *data* through the full LZ77 + Huffman pipeline.

    Returns a self-contained DLZ2 binary blob.
    """
    # 1. LZ77 pass
    lz_bytes = lz77.encode_to_bytes(data, window_size, lookahead_size)
    lz_len = len(lz_bytes)

    # 2. Huffman pass
    if lz_len == 0:
        huff_payload = b""
        lengths = [0] * 256
    else:
        huff_payload, lengths = huffman.encode_bytes(lz_bytes)

    # 3. Build container
    writer = BitWriter()
    writer.write_bytes(MAGIC_V2)
    writer.write_bytes(bytes([HEADER_FLAGS]))
    writer.write_bytes(struct.pack("<Q", len(data)))      # original length
    writer.write_bytes(struct.pack("<Q", lz_len))         # LZ stream length
    huffman.write_lengths(writer, lengths)
    # Align to byte boundary, then append payload
    writer.flush()
    writer.write_bytes(huff_payload)

    return writer.get_bytes()


# -------------------------------------------------------------------
# Decompress
# -------------------------------------------------------------------

def decompress(data: bytes) -> bytes:
    """
    Decompress a DEFLATE-lite container (supports both v1 and v2).
    """
    if data[:4] == MAGIC_V2:
        return _decompress_v2(data)
    elif data[:4] == MAGIC_V1:
        return _decompress_v1(data)
    else:
        raise ValueError(f"Bad magic: {data[:4]!r} (expected DLZ1 or DLZ2)")


def _decompress_v2(data: bytes) -> bytes:
    """Decompress a DLZ2 container."""
    reader = BitReader(data)

    magic = reader.read_bytes(4)
    if magic != MAGIC_V2:
        raise ValueError(f"Bad magic: {magic!r} (expected {MAGIC_V2!r})")

    _flags = reader.read_bytes(1)
    orig_len = struct.unpack("<Q", reader.read_bytes(8))[0]
    lz_len = struct.unpack("<Q", reader.read_bytes(8))[0]
    lengths = huffman.read_lengths(reader)

    # Align to byte boundary
    if not reader.aligned():
        reader.read_bits(8 - reader._bit_pos)
    payload = reader._data[reader._byte_pos:]

    if orig_len == 0:
        return b""

    # Huffman decode: we expect exactly lz_len output bytes
    lz_bytes = huffman.decode_bytes(payload, lengths, lz_len)

    # LZ77 decode
    return lz77.decode_from_bytes(lz_bytes)


def _decompress_v1(data: bytes) -> bytes:
    """
    Decompress a DLZ1 container (v1 — no LZ stream length stored).

    This is a best-effort legacy path.  The codec tries increasing
    symbol counts until the LZ77 stream decodes without error.
    """
    reader = BitReader(data)

    magic = reader.read_bytes(4)
    if magic != MAGIC_V1:
        raise ValueError(f"Bad magic: {magic!r} (expected {MAGIC_V1!r})")

    _flags = reader.read_bytes(1)
    orig_len = struct.unpack("<Q", reader.read_bytes(8))[0]
    lengths = huffman.read_lengths(reader)

    if not reader.aligned():
        reader.read_bits(8 - reader._bit_pos)
    payload = reader._data[reader._byte_pos:]

    if orig_len == 0:
        return b""

    # In v1 we try to find the right decode length by trial.
    # Start from a conservative estimate and try up.
    # The LZ stream is at least orig_len bytes (all literals).
    # Upper bound: each payload byte could encode up to 8 single-bit symbols.
    max_symbols = len(payload) * 8
    if max_symbols == 0:
        return b""

    # Try the max; if too many, shrink by trying the actual bit count
    # of valid codes.  For now, just try decoding max_symbols and
    # truncate the LZ stream to what's valid.
    try:
        lz_bytes = huffman.decode_bytes(payload, lengths, max_symbols)
        return lz77.decode_from_bytes(lz_bytes)
    except (ValueError, EOFError):
        # Binary search for the right length
        lo, hi = orig_len, max_symbols
        best = b""
        while lo <= hi:
            mid = (lo + hi) // 2
            try:
                candidate = huffman.decode_bytes(payload, lengths, mid)
                result = lz77.decode_from_bytes(candidate)
                best = result
                if len(result) >= orig_len:
                    break
                lo = mid + 1
            except (ValueError, EOFError):
                hi = mid - 1
        return best


# -------------------------------------------------------------------
# Convenience aliases
# -------------------------------------------------------------------

def compress_file(data: bytes, **kwargs) -> bytes:
    """Alias for compress (v2)."""
    return compress(data, **kwargs)


def decompress_file(data: bytes) -> bytes:
    """Alias for decompress.  Handles both v1 and v2 containers."""
    return decompress(data)
