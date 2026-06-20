"""
Bitstream I/O utilities for the DEFLATE-lite codec.

Provides BitWriter (write individual bits into a byte buffer) and
BitReader (read individual bits back).  Bits are packed LSB-first within
each byte, matching the DEFLATE convention.
"""

from __future__ import annotations

import io


class BitWriter:
    """Write individual bits to an in-memory byte buffer (LSB-first packing)."""

    def __init__(self) -> None:
        self._buf = bytearray()
        self._current_byte: int = 0
        self._bit_pos: int = 0  # next bit position in _current_byte (0..7)

    # ------------------------------------------------------------------
    # Low-level API
    # ------------------------------------------------------------------

    def write_bit(self, bit: int) -> None:
        """Append a single bit (0 or 1)."""
        if bit:
            self._current_byte |= 1 << self._bit_pos
        self._bit_pos += 1
        if self._bit_pos == 8:
            self._flush_byte()

    def write_bits(self, value: int, n_bits: int) -> None:
        """
        Write *n_bits* bits from *value* (LSB-first).

        For example, write_bits(0b1011, 4) writes bits 1, 1, 0, 1
        (least-significant first).
        """
        for i in range(n_bits):
            self.write_bit((value >> i) & 1)

    def write_bytes(self, data: bytes) -> None:
        """Write whole bytes (aligned to byte boundary first)."""
        if self._bit_pos != 0:
            self._flush_byte()
        self._buf.extend(data)

    # ------------------------------------------------------------------
    # Finalize
    # ------------------------------------------------------------------

    def flush(self) -> None:
        """Flush any partially-filled byte (pads with zero bits)."""
        if self._bit_pos > 0:
            self._flush_byte()

    def get_bytes(self) -> bytes:
        """Return all written bytes (flushes automatically)."""
        self.flush()
        return bytes(self._buf)

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def _flush_byte(self) -> None:
        self._buf.append(self._current_byte)
        self._current_byte = 0
        self._bit_pos = 0

    def __len__(self) -> int:
        """Return total number of bits written so far."""
        return len(self._buf) * 8 + self._bit_pos


class BitReader:
    """Read individual bits from a bytes object (LSB-first packing)."""

    def __init__(self, data: bytes) -> None:
        self._data = data
        self._byte_pos: int = 0
        self._bit_pos: int = 0  # next bit position in current byte (0..7)

    # ------------------------------------------------------------------
    # Low-level API
    # ------------------------------------------------------------------

    def read_bit(self) -> int:
        """Read and return a single bit (0 or 1). Raises EOFError on exhaustion."""
        if self._byte_pos >= len(self._data):
            raise EOFError("No more bits to read")
        bit = (self._data[self._byte_pos] >> self._bit_pos) & 1
        self._bit_pos += 1
        if self._bit_pos == 8:
            self._bit_pos = 0
            self._byte_pos += 1
        return bit

    def read_bits(self, n_bits: int) -> int:
        """Read *n_bits* bits and return as an integer (LSB-first)."""
        value = 0
        for i in range(n_bits):
            value |= self.read_bit() << i
        return value

    def read_bytes(self, n: int) -> bytes:
        """Read *n* whole bytes (must be on a byte boundary)."""
        if self._bit_pos != 0:
            # Advance to next byte boundary
            self._byte_pos += 1
            self._bit_pos = 0
        end = self._byte_pos + n
        if end > len(self._data):
            raise EOFError("Not enough bytes remaining")
        result = self._data[self._byte_pos : end]
        self._byte_pos = end
        return result

    def remaining_bits(self) -> int:
        """Return the number of unread bits."""
        return (len(self._data) - self._byte_pos) * 8 - self._bit_pos

    def aligned(self) -> bool:
        """True if the reader is on a byte boundary."""
        return self._bit_pos == 0

    def __len__(self) -> int:
        return (len(self._data) - self._byte_pos) * 8 - self._bit_pos
