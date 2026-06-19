"""Integration tests for the CLI entry point."""

import os
import tempfile
from pathlib import Path

from deflate_lite.cli import main


def test_compress_decompress_roundtrip(tmp_path: Path):
    """Full CLI round-trip: compress then decompress, verify equality."""
    original = b"The quick brown fox jumps over the lazy dog. " * 200
    input_file = tmp_path / "input.bin"
    compressed_file = tmp_path / "compressed.dlz"
    output_file = tmp_path / "output.bin"

    input_file.write_bytes(original)

    # Compress
    main(["compress", str(input_file), str(compressed_file)])
    assert compressed_file.exists()
    assert len(compressed_file.read_bytes()) < len(original)

    # Decompress
    main(["decompress", str(compressed_file), str(output_file)])
    assert output_file.exists()
    assert output_file.read_bytes() == original


def test_compress_empty(tmp_path: Path):
    input_file = tmp_path / "empty.bin"
    compressed_file = tmp_path / "empty.dlz"
    output_file = tmp_path / "empty_out.bin"

    input_file.write_bytes(b"")

    main(["compress", str(input_file), str(compressed_file)])
    main(["decompress", str(compressed_file), str(output_file)])
    assert output_file.read_bytes() == b""


def test_info_command(tmp_path: Path, capsys):
    f = tmp_path / "test.txt"
    f.write_bytes(b"hello world" * 100)
    main(["info", str(f)])
    captured = capsys.readouterr()
    assert "1,100" in captured.out
    assert "Shannon entropy" in captured.out


def test_analyze_command(tmp_path: Path, capsys):
    original = tmp_path / "orig.bin"
    compressed = tmp_path / "comp.dlz"
    original.write_bytes(b"AAAA" * 500)
    main(["compress", str(original), str(compressed)])
    main(["analyze", str(original), str(compressed)])
    captured = capsys.readouterr()
    assert "Ratio" in captured.out


def test_compress_custom_window(tmp_path: Path):
    data = b"abcdefghij" * 100
    input_file = tmp_path / "input.bin"
    compressed_file = tmp_path / "compressed.dlz"
    output_file = tmp_path / "output.bin"

    input_file.write_bytes(data)
    main(["compress", str(input_file), str(compressed_file), "--window", "256"])
    main(["decompress", str(compressed_file), str(output_file)])
    assert output_file.read_bytes() == data


def test_compress_random_binary(tmp_path: Path):
    data = os.urandom(2000)
    input_file = tmp_path / "random.bin"
    compressed_file = tmp_path / "random.dlz"
    output_file = tmp_path / "random_out.bin"

    input_file.write_bytes(data)
    main(["compress", str(input_file), str(compressed_file)])
    main(["decompress", str(compressed_file), str(output_file)])
    assert output_file.read_bytes() == data
