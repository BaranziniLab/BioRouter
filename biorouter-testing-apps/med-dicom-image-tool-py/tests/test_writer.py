"""Tests for pure-Python PNG and PGM writers."""

import struct
from pathlib import Path

import pytest

from medicom.writer import (
    write_pgm,
    write_png,
    write_png_from_16bit,
    write_pgm_from_16bit,
)


class TestPGMWriter:
    def test_write_pgm_8bit(self, tmp_path):
        pixels = [0, 128, 255] * 4
        out = write_pgm(pixels, 6, 2, tmp_path / "test.pgm")
        assert out.exists()
        content = out.read_bytes()
        assert content.startswith(b"P5\n6 2\n255\n")

    def test_pgm_pixel_count(self, tmp_path):
        width, height = 4, 3
        pixels = list(range(256))[:width * height]
        out = write_pgm(pixels, width, height, tmp_path / "test.pgm")
        content = out.read_bytes()
        header_end = content.index(b"\n", content.index(b"\n", content.index(b"\n") + 1) + 1) + 1
        pixel_data = content[header_end:]
        assert len(pixel_data) == width * height

    def test_pgm_from_bytes(self, tmp_path):
        pixels = bytes([0, 50, 100, 150, 200, 255])
        out = write_pgm(pixels, 6, 1, tmp_path / "test.pgm")
        assert out.exists()

    def test_pgm_16bit(self, tmp_path):
        pixels = [0, 1000, 4095]
        out = write_pgm(pixels, 3, 1, tmp_path / "test.pgm", max_val=4095)
        content = out.read_bytes()
        assert b"4095" in content

    def test_pgm_creates_parent_dirs(self, tmp_path):
        pixels = [128]
        out = write_pgm(pixels, 1, 1, tmp_path / "sub" / "dir" / "test.pgm")
        assert out.exists()


class TestPNGWriter:
    def test_write_png(self, tmp_path):
        pixels = [0, 128, 255] * 4
        out = write_png(pixels, 6, 2, tmp_path / "test.png")
        assert out.exists()
        content = out.read_bytes()
        # PNG signature
        assert content[:8] == b"\x89PNG\r\n\x1a\n"

    def test_png_pixel_count(self, tmp_path):
        width, height = 4, 3
        pixels = list(range(256))[:width * height]
        out = write_png(pixels, width, height, tmp_path / "test.png")
        assert out.exists()
        assert out.stat().st_size > 0

    def test_png_from_bytes(self, tmp_path):
        pixels = bytes([0, 50, 100, 150, 200, 255])
        out = write_png(pixels, 6, 1, tmp_path / "test.png")
        assert out.exists()

    def test_png_creates_parent_dirs(self, tmp_path):
        pixels = [128]
        out = write_png(pixels, 1, 1, tmp_path / "sub" / "dir" / "test.png")
        assert out.exists()


class Test16BitWriters:
    def test_png_from_16bit(self, tmp_path):
        pixels = struct.pack("<4H", 0, 1000, 2000, 4095)
        out = write_png_from_16bit(pixels, 4, 1, tmp_path / "test.png", bits_stored=12)
        assert out.exists()

    def test_pgm_from_16bit(self, tmp_path):
        pixels = struct.pack("<4H", 0, 1000, 2000, 4095)
        out = write_pgm_from_16bit(pixels, 4, 1, tmp_path / "test.pgm", bits_stored=12)
        assert out.exists()

    def test_16bit_from_list(self, tmp_path):
        pixels = [0, 1000, 2000, 4095]
        out = write_png_from_16bit(pixels, 4, 1, tmp_path / "test.png", bits_stored=12)
        assert out.exists()
