"""
Canonical Huffman coding.

Builds optimal prefix-free codes from byte-frequency tables and
encodes/decodes data using a BitWriter/BitReader.

Canonical Huffman codes are written and decoded MSB-first (the standard
convention).  The BitWriter/BitReader pack bits LSB-first within each
byte, but individual Huffman code symbols are emitted MSB-first so
prefix-freeness is preserved.
"""

from __future__ import annotations

import heapq
from typing import Dict, List, Optional, Tuple

from deflate_lite.bitio import BitReader, BitWriter


# -------------------------------------------------------------------
# Huffman tree node
# -------------------------------------------------------------------

class _Node:
    __slots__ = ("freq", "left", "right", "symbol")

    def __init__(
        self,
        freq: int,
        left: Optional["_Node"] = None,
        right: Optional["_Node"] = None,
        symbol: Optional[int] = None,
    ):
        self.freq = freq
        self.left = left
        self.right = right
        self.symbol = symbol

    def __lt__(self, other: "_Node") -> bool:
        return self.freq < other.freq


# -------------------------------------------------------------------
# Build code lengths from frequencies
# -------------------------------------------------------------------

def build_code_lengths(freqs: List[int]) -> List[int]:
    """
    Given a list of 256 byte frequencies, return a list of 256 code
    lengths.  Symbols with zero frequency get length 0.
    """
    active = [(f, i) for i, f in enumerate(freqs) if f > 0]

    if len(active) == 0:
        return [0] * 256

    if len(active) == 1:
        lengths = [0] * 256
        lengths[active[0][1]] = 1
        return lengths

    heap: List[_Node] = []
    for f, sym in active:
        heapq.heappush(heap, _Node(f, symbol=sym))

    while len(heap) > 1:
        left = heapq.heappop(heap)
        right = heapq.heappop(heap)
        parent = _Node(left.freq + right.freq, left=left, right=right)
        heapq.heappush(heap, parent)

    root = heap[0]
    lengths = [0] * 256

    def _walk(node: _Node, depth: int) -> None:
        if node.symbol is not None:
            lengths[node.symbol] = depth
            return
        if node.left is not None:
            _walk(node.left, depth + 1)
        if node.right is not None:
            _walk(node.right, depth + 1)

    _walk(root, 0)
    return lengths


def _limit_code_lengths(lengths: List[int], max_bits: int = 15) -> List[int]:
    """Limit code lengths to *max_bits* (DEFLATE uses 15)."""
    return [min(l, max_bits) for l in lengths]


# -------------------------------------------------------------------
# Canonical codes
# -------------------------------------------------------------------

def canonical_codes_from_lengths(lengths: List[int]) -> Dict[int, Tuple[int, int]]:
    """
    Convert code lengths to canonical Huffman codes.

    Returns a dict mapping symbol -> (code_value, code_length).
    Canonical codes are assigned in ascending symbol order for the
    same length, starting from 0 for each new length.

    The code_value is an integer whose *MSB-first* bit pattern
    (bit n-1 first, bit 0 last) is the canonical code.
    """
    pairs = sorted((l, s) for s, l in enumerate(lengths) if l > 0)
    if not pairs:
        return {}

    codes: Dict[int, Tuple[int, int]] = {}
    code = 0
    prev_len = pairs[0][0]

    for length, symbol in pairs:
        while prev_len < length:
            code <<= 1
            prev_len += 1
        codes[symbol] = (code, length)
        code += 1

    return codes


# -------------------------------------------------------------------
# Encode bytes
# -------------------------------------------------------------------

def _write_code(writer: BitWriter, value: int, nbits: int) -> None:
    """Write a Huffman code value MSB-first (bit n-1 first, bit 0 last)."""
    for i in range(nbits - 1, -1, -1):
        writer.write_bit((value >> i) & 1)


def encode_bytes(data: bytes) -> Tuple[bytes, List[int]]:
    """
    Huffman-encode *data*.

    Returns (compressed_bits, code_lengths_256).
    """
    if not data:
        return (b"", [0] * 256)

    freqs = [0] * 256
    for b in data:
        freqs[b] += 1

    lengths = _limit_code_lengths(build_code_lengths(freqs))
    codes = canonical_codes_from_lengths(lengths)

    writer = BitWriter()
    for b in data:
        val, nbits = codes[b]
        _write_code(writer, val, nbits)

    return (writer.get_bytes(), lengths)


# -------------------------------------------------------------------
# Decode bytes
# -------------------------------------------------------------------

def decode_bytes(compressed: bytes, lengths: List[int], original_length: int) -> bytes:
    """
    Huffman-decode *compressed* bits using the given *lengths* table.

    *original_length* is the expected number of output bytes (needed
    because the bitstream has no end-of-stream marker).
    """
    if original_length == 0:
        return b""

    codes = canonical_codes_from_lengths(lengths)

    # Build a decode trie in MSB-first bit order.
    #
    # Canonical code values are interpreted MSB-first: for value 0b101
    # with nbits=3, the bit sequence read from the stream is 1, 0, 1
    # (bit 2 first, then bit 1, then bit 0).
    trie: dict = {}
    for sym, (val, nbits) in codes.items():
        node = trie
        for bit_idx in range(nbits - 1, -1, -1):  # MSB-first
            bit = (val >> bit_idx) & 1
            if bit not in node:
                node[bit] = {}
            node = node[bit]
        node["sym"] = sym

    reader = BitReader(compressed)
    result = bytearray()
    for _ in range(original_length):
        node = trie
        while "sym" not in node:
            bit = reader.read_bit()
            if bit not in node:
                raise ValueError(
                    f"Invalid Huffman code at bit position "
                    f"{reader._byte_pos * 8 + reader._bit_pos}"
                )
            node = node[bit]
        result.append(node["sym"])

    return bytes(result)


# -------------------------------------------------------------------
# Serialise / deserialise code-length table
# -------------------------------------------------------------------

def write_lengths(writer: BitWriter, lengths: List[int]) -> None:
    """Write 256 code-lengths to the bitstream (each as 4 bits, 0-15)."""
    for l in lengths:
        writer.write_bits(l, 4)


def read_lengths(reader: BitReader) -> List[int]:
    """Read 256 code-lengths from the bitstream."""
    return [reader.read_bits(4) for _ in range(256)]
