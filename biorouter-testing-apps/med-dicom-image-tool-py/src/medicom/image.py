"""Image operations on DICOM pixel arrays.

Provides:
  - Windowing / leveling to 8-bit
  - CT Hounsfield Unit rescale
  - Intensity statistics
  - Simple thresholding / segmentation
  - Histogram computation

All functions work on raw pixel arrays (Python lists or numpy-free).
"""

from __future__ import annotations

import struct
from collections import Counter
from dataclasses import dataclass
from typing import Dict, List, Optional, Tuple, Union


# ── Windowing / Leveling ─────────────────────────────────────────────────────

def apply_window(
    pixels: Union[bytes, List[int]],
    window_center: float,
    window_width: float,
    bits_stored: int = 12,
    pixel_representation: int = 0,
) -> List[int]:
    """Apply window/level transformation to produce 8-bit grayscale output.

    Parameters
    ----------
    pixels : raw pixel values (unsigned integers)
    window_center : display window center
    window_width : display window width
    bits_stored : number of stored bits per pixel
    pixel_representation : 0 = unsigned, 1 = signed

    Returns
    -------
    List of 8-bit values (0–255) suitable for PGM/PPM output.
    """
    max_stored = (2 ** bits_stored) - 1
    min_val = 0
    max_val = max_stored
    if pixel_representation == 1:
        min_val = -(2 ** (bits_stored - 1))
        max_val = (2 ** (bits_stored - 1)) - 1

    # Window bounds
    win_min = window_center - window_width / 2
    win_max = window_center + window_width / 2

    output: List[int] = []

    if isinstance(pixels, bytes):
        count = len(pixels) // 2
        values = struct.unpack(f"<{count}H", pixels)
    else:
        values = pixels

    # Convert signed if needed
    if pixel_representation == 1:
        values = [v if v < (2 ** 15) else v - (2 ** 16) for v in values]

    for px in values:
        if px <= win_min:
            output.append(0)
        elif px >= win_max:
            output.append(255)
        else:
            normalized = (px - win_min) / (win_max - win_min)
            output.append(int(normalized * 255 + 0.5))

    return output


def window_width_height_to_8bit(
    pixels: Union[bytes, List[int]],
    window_center: float,
    window_width: float,
    slope: float = 1.0,
    intercept: float = 0.0,
    bits_stored: int = 12,
    pixel_representation: int = 0,
) -> List[int]:
    """Apply window/level with optional rescale slope/intercept to 8-bit.

    First rescales stored values to real values (slope * stored + intercept),
    then applies window/level on the rescaled values.
    """
    if isinstance(pixels, bytes):
        count = len(pixels) // 2
        stored = list(struct.unpack(f"<{count}H", pixels))
    else:
        stored = list(pixels)

    if pixel_representation == 1:
        stored = [v if v < (2 ** 15) else v - (2 ** 16) for v in stored]

    # Rescale to real values
    rescaled = [slope * v + intercept for v in stored]

    # Apply window on rescaled values
    win_min = window_center - window_width / 2
    win_max = window_center + window_width / 2

    output: List[int] = []
    for px in rescaled:
        if px <= win_min:
            output.append(0)
        elif px >= win_max:
            output.append(255)
        else:
            normalized = (px - win_min) / (win_max - win_min)
            output.append(int(normalized * 255 + 0.5))

    return output


# ── Hounsfield Unit rescale ──────────────────────────────────────────────────

def rescale_to_hu(
    pixels: Union[bytes, List[int]],
    slope: float = 1.0,
    intercept: float = 0.0,
    bits_stored: int = 12,
    pixel_representation: int = 0,
) -> List[float]:
    """Convert stored pixel values to Hounsfield Units.

    HU = slope * stored_value + intercept

    For CT: intercept is typically -1024 (air = -1000 HU, water = 0 HU).
    """
    if isinstance(pixels, bytes):
        count = len(pixels) // 2
        stored = list(struct.unpack(f"<{count}H", pixels))
    else:
        stored = list(pixels)

    if pixel_representation == 1:
        stored = [v if v < (2 ** 15) else v - (2 ** 16) for v in stored]

    return [slope * v + intercept for v in stored]


def hu_to_pixel(
    hu_value: float,
    slope: float = 1.0,
    intercept: float = 0.0,
    bits_stored: int = 12,
) -> int:
    """Convert a Hounsfield Unit value back to a stored pixel value."""
    stored = (hu_value - intercept) / slope
    max_val = (2 ** bits_stored) - 1
    return max(0, min(int(stored + 0.5), max_val))


# ── Intensity statistics ─────────────────────────────────────────────────────

@dataclass
class IntensityStats:
    """Basic intensity statistics for a pixel array."""
    count: int
    min: float
    max: float
    mean: float
    std: float
    median: float
    p5: float
    p95: float


def intensity_stats(
    pixels: Union[bytes, List[int]],
    bits_stored: int = 12,
    pixel_representation: int = 0,
) -> IntensityStats:
    """Compute basic intensity statistics."""
    if isinstance(pixels, bytes):
        count = len(pixels) // 2
        values = list(struct.unpack(f"<{count}H", pixels))
    else:
        values = list(pixels)

    if pixel_representation == 1:
        values = [v if v < (2 ** 15) else v - (2 ** 16) for v in values]

    if not values:
        return IntensityStats(0, 0, 0, 0, 0, 0, 0, 0)

    n = len(values)
    sorted_vals = sorted(values)
    mn = sorted_vals[0]
    mx = sorted_vals[-1]
    mean = sum(values) / n
    variance = sum((v - mean) ** 2 for v in values) / max(n - 1, 1)
    std = variance ** 0.5
    median = sorted_vals[n // 2] if n % 2 else (sorted_vals[n // 2 - 1] + sorted_vals[n // 2]) / 2

    p5_idx = max(0, int(n * 0.05))
    p95_idx = min(n - 1, int(n * 0.95))

    return IntensityStats(
        count=n,
        min=mn,
        max=mx,
        mean=mean,
        std=std,
        median=median,
        p5=sorted_vals[p5_idx],
        p95=sorted_vals[p95_idx],
    )


# ── Histogram ────────────────────────────────────────────────────────────────

def histogram(
    pixels: Union[bytes, List[int]],
    num_bins: int = 256,
    bits_stored: int = 12,
    pixel_representation: int = 0,
) -> Dict[int, int]:
    """Compute a histogram with auto-binning.

    Returns a dict mapping bin index (0..num_bins-1) to count.
    """
    if isinstance(pixels, bytes):
        count = len(pixels) // 2
        values = list(struct.unpack(f"<{count}H", pixels))
    else:
        values = list(pixels)

    if pixel_representation == 1:
        values = [v if v < (2 ** 15) else v - (2 ** 16) for v in values]

    if not values:
        return {}

    min_val = min(values)
    max_val = max(values)
    range_val = max_val - min_val

    if range_val == 0:
        return {num_bins // 2: len(values)}

    bin_width = range_val / num_bins
    hist: Dict[int, int] = Counter()

    for v in values:
        bin_idx = int((v - min_val) / bin_width)
        bin_idx = min(bin_idx, num_bins - 1)
        hist[bin_idx] += 1

    return dict(hist)


# ── Thresholding / Segmentation ──────────────────────────────────────────────

def threshold(
    pixels: Union[bytes, List[int]],
    low: float,
    high: float,
    bits_stored: int = 12,
    pixel_representation: int = 0,
) -> List[int]:
    """Binary segmentation: pixels in [low, high] → 1, else → 0.

    Returns a list of 0/1 values.
    """
    if isinstance(pixels, bytes):
        count = len(pixels) // 2
        values = list(struct.unpack(f"<{count}H", pixels))
    else:
        values = list(pixels)

    if pixel_representation == 1:
        values = [v if v < (2 ** 15) else v - (2 ** 16) for v in values]

    return [1 if low <= v <= high else 0 for v in values]


def threshold_hu(
    pixels: Union[bytes, List[int]],
    low_hu: float,
    high_hu: float,
    slope: float = 1.0,
    intercept: float = 0.0,
    bits_stored: int = 12,
    pixel_representation: int = 0,
) -> List[int]:
    """Binary segmentation on HU range.

    Converts stored values to HU, then thresholds in [low_hu, high_hu].
    """
    hu_values = rescale_to_hu(pixels, slope, intercept, bits_stored, pixel_representation)
    return [1 if low_hu <= v <= high_hu else 0 for v in hu_values]


def segmentation_area(
    mask: List[int],
    pixel_spacing: Optional[Tuple[float, float]] = None,
) -> float:
    """Compute area of a binary segmentation mask.

    If pixel_spacing is provided (row_spacing, col_spacing) in mm,
    returns area in mm². Otherwise returns pixel count.
    """
    pixel_count = sum(mask)
    if pixel_spacing is not None:
        row_sp, col_sp = pixel_spacing
        return pixel_count * row_sp * col_sp
    return float(pixel_count)


def segmentation_fraction(
    mask: List[int],
    total: Optional[int] = None,
) -> float:
    """Compute fraction of foreground pixels in a mask."""
    if total is None:
        total = len(mask)
    if total == 0:
        return 0.0
    return sum(mask) / total


# ── Conversion helpers ───────────────────────────────────────────────────────

def pixels_to_bytes(pixels: Union[List[int], bytes]) -> bytes:
    """Convert a list of 16-bit unsigned pixel values to bytes."""
    if isinstance(pixels, bytes):
        return pixels
    return struct.pack(f"<{len(pixels)}H", *pixels)


def bytes_to_pixels(data: bytes) -> List[int]:
    """Convert raw bytes to a list of 16-bit unsigned pixel values."""
    count = len(data) // 2
    return list(struct.unpack(f"<{count}H", data))


def pixels_from_signed_bytes(
    data: bytes,
    bits_stored: int = 16,
) -> List[int]:
    """Convert raw bytes to signed pixel values based on bits_stored."""
    count = len(data) // 2
    unsigned = list(struct.unpack(f"<{count}H", data))
    if bits_stored <= 16:
        threshold_val = 2 ** (bits_stored - 1)
        return [v - 2 ** bits_stored if v >= threshold_val else v for v in unsigned]
    return unsigned
