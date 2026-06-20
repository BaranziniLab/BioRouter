"""Tests for image operations — windowing, HU rescale, segmentation, stats."""

import struct
from typing import List

import pytest

from medicom.image import (
    apply_window,
    window_width_height_to_8bit,
    rescale_to_hu,
    hu_to_pixel,
    intensity_stats,
    histogram,
    threshold,
    threshold_hu,
    segmentation_area,
    segmentation_fraction,
    pixels_to_bytes,
    bytes_to_pixels,
)
from medicom.generate import generate_dicom


# ── Helpers ──────────────────────────────────────────────────────────────────

def _make_uint16_pixels(values: List[int]) -> bytes:
    """Pack a list of ints into uint16 LE bytes."""
    return struct.pack(f"<{len(values)}H", *values)


# ── Windowing tests ──────────────────────────────────────────────────────────

class TestWindowing:
    """Window/level math correctness."""

    def test_window_full_width(self):
        """Full-width window should map min→0, max→255."""
        pixels = _make_uint16_pixels([0, 100, 200, 4095])
        result = apply_window(pixels, window_center=2047.5, window_width=4095, bits_stored=12)
        assert result[0] == 0
        assert result[-1] == 255

    def test_window_narrow(self):
        """Narrow window should saturate extremes."""
        pixels = _make_uint16_pixels([0, 100, 200, 400, 1000])
        result = apply_window(pixels, window_center=200, window_width=100, bits_stored=12)
        # Below window → 0
        assert result[0] == 0
        assert result[1] == 0
        # At center → ~128
        assert 120 <= result[2] <= 135
        # Above window → 255
        assert result[-1] == 255

    def test_window_edge_values(self):
        """Window edge values should map to 0 and 255."""
        pixels = _make_uint16_pixels([100, 300])
        # Window: center=200, width=400 → min=0, max=400
        result = apply_window(pixels, window_center=200, window_width=400, bits_stored=12)
        # 100 is at 100/400 = 25% → ~64
        assert 60 <= result[0] <= 70
        # 300 is at 300/400 = 75% → ~192
        assert 188 <= result[1] <= 196

    def test_window_monotonic(self):
        """Output should be monotonically non-decreasing for increasing input."""
        pixels = _make_uint16_pixels(list(range(0, 4096, 64)))
        result = apply_window(pixels, window_center=2048, window_width=4096, bits_stored=12)
        for i in range(1, len(result)):
            assert result[i] >= result[i-1], f"Non-monotonic at index {i}"

    def test_window_list_input(self):
        """Should work with a list of ints as well."""
        pixels = [0, 100, 200, 4095]
        result = apply_window(pixels, window_center=2047.5, window_width=4095, bits_stored=12)
        assert result[0] == 0
        assert result[-1] == 255

    def test_window_width_height_with_rescale(self):
        """Window/level with rescale slope/intercept."""
        # Stored value 1024 → HU = 1*1024 + (-1024) = 0 HU (water)
        pixels = _make_uint16_pixels([0, 1024, 2048, 3072])
        # HU values: -1024, 0, 1024, 2048
        # Window center=0, width=1000 → HU range [-500, 500]
        result = window_width_height_to_8bit(
            pixels,
            window_center=0, window_width=1000,
            slope=1.0, intercept=-1024.0,
            bits_stored=12,
        )
        # HU=-1024 → below window → 0
        assert result[0] == 0
        # HU=0 → at center → ~128
        assert 120 <= result[1] <= 136
        # HU=1024 → above window → 255
        assert result[2] == 255
        assert result[3] == 255


# ── HU rescale tests ────────────────────────────────────────────────────────

class TestHURescale:
    def test_ct_rescale_air(self):
        """Stored value 0 with intercept=-1024 → HU=-1024 (air)."""
        pixels = _make_uint16_pixels([0])
        hu = rescale_to_hu(pixels, slope=1.0, intercept=-1024.0)
        assert hu[0] == pytest.approx(-1024.0)

    def test_ct_rescale_water(self):
        """Stored value 1024 → HU=0 (water)."""
        pixels = _make_uint16_pixels([1024])
        hu = rescale_to_hu(pixels, slope=1.0, intercept=-1024.0)
        assert hu[0] == pytest.approx(0.0)

    def test_ct_rescale_soft_tissue(self):
        """Stored value ~1064 → HU=40 (soft tissue)."""
        pixels = _make_uint16_pixels([1064])
        hu = rescale_to_hu(pixels, slope=1.0, intercept=-1024.0)
        assert hu[0] == pytest.approx(40.0)

    def test_ct_rescale_with_slope(self):
        """Non-unity slope."""
        pixels = _make_uint16_pixels([100])
        hu = rescale_to_hu(pixels, slope=2.0, intercept=-1000.0)
        # HU = 2*100 + (-1000) = -800
        assert hu[0] == pytest.approx(-800.0)

    def test_roundtrip_hu_to_pixel(self):
        """Convert HU → stored → HU should be approximately identity."""
        hu_in = 40.0
        stored = hu_to_pixel(hu_in, slope=1.0, intercept=-1024.0, bits_stored=12)
        # stored = (40 - (-1024)) / 1 = 1064
        assert stored == 1064
        pixels = _make_uint16_pixels([stored])
        hu_out = rescale_to_hu(pixels, slope=1.0, intercept=-1024.0)
        assert hu_out[0] == pytest.approx(hu_in)


# ── Intensity statistics tests ───────────────────────────────────────────────

class TestIntensityStats:
    def test_uniform_pixels(self):
        pixels = _make_uint16_pixels([100] * 100)
        stats = intensity_stats(pixels, bits_stored=12)
        assert stats.count == 100
        assert stats.min == 100
        assert stats.max == 100
        assert stats.mean == pytest.approx(100.0)
        assert stats.std == pytest.approx(0.0)

    def test_gradient_pixels(self):
        pixels = _make_uint16_pixels(list(range(0, 100)))
        stats = intensity_stats(pixels, bits_stored=12)
        assert stats.count == 100
        assert stats.min == 0
        assert stats.max == 99
        assert stats.mean == pytest.approx(49.5)

    def test_empty_pixels(self):
        stats = intensity_stats(_make_uint16_pixels([]), bits_stored=12)
        assert stats.count == 0


# ── Histogram tests ─────────────────────────────────────────────────────────

class TestHistogram:
    def test_uniform_histogram(self):
        pixels = _make_uint16_pixels([500] * 100)
        hist = histogram(pixels, num_bins=256, bits_stored=12)
        # All in one bin
        assert sum(hist.values()) == 100
        assert any(v == 100 for v in hist.values())

    def test_histogram_count(self):
        pixels = _make_uint16_pixels(list(range(0, 4096, 16)))
        hist = histogram(pixels, num_bins=256, bits_stored=12)
        assert sum(hist.values()) == len(range(0, 4096, 16))

    def test_histogram_empty(self):
        hist = histogram(_make_uint16_pixels([]), bits_stored=12)
        assert len(hist) == 0


# ── Segmentation tests ──────────────────────────────────────────────────────

class TestSegmentation:
    def test_threshold_basic(self):
        """Threshold [100, 200] should mark 150 as 1, others as 0."""
        pixels = [50, 100, 150, 200, 250]
        mask = threshold(pixels, low=100, high=200)
        assert mask == [0, 1, 1, 1, 0]

    def test_threshold_hu_soft_tissue(self):
        """HU threshold for soft tissue [20, 80]."""
        # Stored values for HU: 0→-1024, 1024→0, 1064→40, 1104→80
        pixels = _make_uint16_pixels([0, 1024, 1064, 1104, 1200])
        mask = threshold_hu(
            pixels, low_hu=20, high_hu=80,
            slope=1.0, intercept=-1024.0,
        )
        assert mask == [0, 0, 1, 1, 0]

    def test_segmentation_area_no_spacing(self):
        mask = [1, 1, 0, 1, 0, 1]
        assert segmentation_area(mask) == 4.0

    def test_segmentation_area_with_spacing(self):
        mask = [1, 1, 1, 1]
        area = segmentation_area(mask, pixel_spacing=(0.5, 0.5))
        assert area == pytest.approx(1.0)

    def test_segmentation_fraction(self):
        mask = [1, 0, 1, 0, 1]
        assert segmentation_fraction(mask) == pytest.approx(0.6)


# ── Conversion tests ─────────────────────────────────────────────────────────

class TestConversion:
    def test_pixels_to_bytes_roundtrip(self):
        values = [0, 100, 1000, 4095]
        raw = pixels_to_bytes(values)
        recovered = bytes_to_pixels(raw)
        assert recovered == values

    def test_empty_conversion(self):
        assert pixels_to_bytes([]) == b""
        assert bytes_to_pixels(b"") == []


# ── Integration: parse + window from generated file ──────────────────────────

class TestIntegration:
    def test_parse_window_write(self, tmp_path):
        """Full pipeline: generate → parse → window → write PNG."""
        from medicom.dicom.reader import DICOMFile
        from medicom.dicom.tags import ROWS, COLUMNS, BITS_STORED, WINDOW_CENTER, WINDOW_WIDTH
        from medicom.writer import write_png

        dcm_path = generate_dicom(
            output=tmp_path / "test.dcm",
            rows=8, cols=8,
            pixel_pattern="checker",
        )
        dcm = DICOMFile.from_path(dcm_path)
        rows = dcm.dataset.get_int(ROWS)
        cols = dcm.dataset.get_int(COLUMNS)

        raw = dcm.pixel_array()
        wc = float(dcm.dataset.get_str(WINDOW_CENTER))
        ww = float(dcm.dataset.get_str(WINDOW_WIDTH))
        bits = dcm.dataset.get_int(BITS_STORED)

        windowed = apply_window(raw, wc, ww, bits_stored=bits)
        assert len(windowed) == rows * cols
        assert all(0 <= v <= 255 for v in windowed)

        out_png = tmp_path / "test.png"
        write_png(windowed, cols, rows, out_png)
        assert out_png.exists()
        assert out_png.stat().st_size > 0

        out_pgm = tmp_path / "test.pgm"
        from medicom.writer import write_pgm
        write_pgm(windowed, cols, rows, out_pgm)
        assert out_pgm.exists()
        assert out_pgm.stat().st_size > 0
