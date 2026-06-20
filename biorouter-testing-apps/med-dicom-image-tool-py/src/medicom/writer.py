"""Pure-Python image writers — PNG and PGM.

Writes grayscale images to:
  - PGM (P5 binary) — simplest possible format, no compression
  - PNG (uncompressed deflate) — uses stdlib zlib for compression

No external dependencies (no PIL, no Pillow).
"""

from __future__ import annotations

import struct
import zlib
from pathlib import Path
from typing import List, Union


# ── PGM Writer ───────────────────────────────────────────────────────────────

def write_pgm(
    pixels: Union[List[int], bytes],
    width: int,
    height: int,
    output: Union[str, Path],
    max_val: int = 255,
) -> Path:
    """Write a grayscale image as PGM (P5 binary format).

    Parameters
    ----------
    pixels : flat list of pixel values (0..max_val)
    width, height : image dimensions
    output : file path
    max_val : maximum pixel value (default 255 for 8-bit)

    Returns
    -------
    Path to the written file.
    """
    output = Path(output)
    output.parent.mkdir(parents=True, exist_ok=True)

    if isinstance(pixels, bytes):
        if max_val <= 255:
            pixel_data = pixels
        else:
            count = len(pixels) // 2
            pixel_data = struct.pack(f"<{count}H", *struct.unpack(f"<{count}H", pixels))
    else:
        if max_val <= 255:
            pixel_data = bytes(max(0, min(255, int(p))) for p in pixels)
        else:
            pixel_data = struct.pack(f"<{len(pixels)}H", *pixels)

    with open(output, "wb") as f:
        f.write(f"P5\n{width} {height}\n{max_val}\n".encode("ascii"))
        f.write(pixel_data)

    return output


# ── PNG Writer ───────────────────────────────────────────────────────────────

def _crc32(data: bytes) -> bytes:
    """Compute CRC32 for PNG chunk."""
    return struct.pack(">I", zlib.crc32(data) & 0xFFFFFFFF)


def _png_chunk(chunk_type: bytes, data: bytes) -> bytes:
    """Build a PNG chunk: length + type + data + CRC."""
    length = struct.pack(">I", len(data))
    return length + chunk_type + data + _crc32(chunk_type + data)


def write_png(
    pixels: Union[List[int], bytes],
    width: int,
    height: int,
    output: Union[str, Path],
) -> Path:
    """Write a grayscale image as PNG using pure Python + zlib.

    Parameters
    ----------
    pixels : flat list of 8-bit pixel values (0–255)
    width, height : image dimensions
    output : file path

    Returns
    -------
    Path to the written file.
    """
    output = Path(output)
    output.parent.mkdir(parents=True, exist_ok=True)

    if isinstance(pixels, bytes):
        pixel_data = pixels
    else:
        pixel_data = bytes(max(0, min(255, int(p))) for p in pixels)

    # ── PNG signature ────────────────────────────────────────────────────
    signature = b"\x89PNG\r\n\x1a\n"

    # ── IHDR chunk ───────────────────────────────────────────────────────
    ihdr_data = struct.pack(">IIBBBBB", width, height, 8, 0, 0, 0, 0)
    # Bit depth 8, color type 0 (grayscale), compression 0, filter 0, interlace 0
    ihdr = _png_chunk(b"IHDR", ihdr_data)

    # ── Raw image data ───────────────────────────────────────────────────
    # PNG requires filter byte (0) at start of each row
    raw_rows = bytearray()
    row_bytes = width  # 1 byte per pixel (grayscale, 8-bit)
    for y in range(height):
        raw_rows.append(0)  # filter: None
        start = y * row_bytes
        end = start + row_bytes
        raw_rows.extend(pixel_data[start:end])

    # Compress with zlib
    compressed = zlib.compress(bytes(raw_rows), 9)
    idat = _png_chunk(b"IDAT", compressed)

    # ── IEND chunk ───────────────────────────────────────────────────────
    iend = _png_chunk(b"IEND", b"")

    with open(output, "wb") as f:
        f.write(signature)
        f.write(ihdr)
        f.write(idat)
        f.write(iend)

    return output


# ── Convenience: write from 16-bit pixels with auto-scaling ──────────────────

def write_png_from_16bit(
    pixels: Union[List[int], bytes],
    width: int,
    height: int,
    output: Union[str, Path],
    bits_stored: int = 12,
    pixel_representation: int = 0,
) -> Path:
    """Write 16-bit DICOM pixels as 8-bit PNG.

    Auto-scales from stored range to 0–255.
    """
    if isinstance(pixels, bytes):
        count = len(pixels) // 2
        values = list(struct.unpack(f"<{count}H", pixels))
    else:
        values = list(pixels)

    if pixel_representation == 1:
        values = [v if v < (2 ** 15) else v - (2 ** 16) for v in values]

    max_stored = (2 ** bits_stored) - 1
    min_val = 0
    if pixel_representation == 1:
        min_val = -(2 ** (bits_stored - 1))
        max_val = (2 ** (bits_stored - 1)) - 1
    else:
        max_val = max_stored

    range_val = max_val - min_val
    if range_val == 0:
        eight_bit = [128] * len(values)
    else:
        eight_bit = [int(((v - min_val) / range_val) * 255 + 0.5) for v in values]

    return write_png(eight_bit, width, height, output)


def write_pgm_from_16bit(
    pixels: Union[List[int], bytes],
    width: int,
    height: int,
    output: Union[str, Path],
    bits_stored: int = 12,
    pixel_representation: int = 0,
) -> Path:
    """Write 16-bit DICOM pixels as PGM (auto-scaled to 8-bit)."""
    if isinstance(pixels, bytes):
        count = len(pixels) // 2
        values = list(struct.unpack(f"<{count}H", pixels))
    else:
        values = list(pixels)

    if pixel_representation == 1:
        values = [v if v < (2 ** 15) else v - (2 ** 16) for v in values]

    max_stored = (2 ** bits_stored) - 1
    min_val = 0
    max_val = max_stored
    if pixel_representation == 1:
        min_val = -(2 ** (bits_stored - 1))
        max_val = (2 ** (bits_stored - 1)) - 1

    range_val = max_val - min_val
    if range_val == 0:
        eight_bit = [128] * len(values)
    else:
        eight_bit = [int(((v - min_val) / range_val) * 255 + 0.5) for v in values]

    return write_pgm(eight_bit, width, height, output)
