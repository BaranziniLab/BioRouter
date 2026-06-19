"""Tests for the CLI module."""

from __future__ import annotations

import os
import tempfile

import pytest

from strmatch.cli import build_parser, main


@pytest.fixture
def sample_text_file(tmp_path):
    """Create a temporary text file for CLI tests."""
    path = tmp_path / "sample.txt"
    path.write_text("ABABABABAB\nhello world\nABAB\n")
    return str(path)


@pytest.fixture
def sample_pattern_file(tmp_path):
    """Create a temporary pattern file."""
    path = tmp_path / "patterns.txt"
    path.write_text("ABAB\nhello\n")
    return str(path)


class TestBuildParser:
    def test_search_command(self):
        parser = build_parser()
        args = parser.parse_args(["search", "ABAB", "file.txt"])
        assert args.command == "search"
        assert args.pattern == "ABAB"
        assert args.file == "file.txt"
        assert args.algo == "kmp"

    def test_search_with_algo(self):
        parser = build_parser()
        args = parser.parse_args(["search", "pat", "file.txt", "--algo", "boyer-moore"])
        assert args.algo == "boyer-moore"

    def test_compare_command(self):
        parser = build_parser()
        args = parser.parse_args(["compare", "pat", "file.txt"])
        assert args.command == "compare"
        assert args.repeats == 3


class TestSearchCommand:
    def test_search_basic(self, sample_text_file, capsys):
        main(["search", "ABAB", sample_text_file])
        out = capsys.readouterr().out
        assert "0" in out

    def test_search_with_time(self, sample_text_file, capsys):
        main(["search", "ABAB", sample_text_file, "--time"])
        out = capsys.readouterr().out
        assert "Time:" in out

    def test_search_with_count(self, sample_text_file, capsys):
        main(["search", "hello", sample_text_file, "--count"])
        out = capsys.readouterr().out
        assert "Count:" in out

    def test_search_no_match(self, sample_text_file, capsys):
        main(["search", "ZZZZZ", sample_text_file])
        out = capsys.readouterr().out.strip()
        # No positions printed (only empty lines).
        lines = [l for l in out.splitlines() if l.strip()]
        assert len(lines) == 0

    def test_search_multi_pattern(self, sample_text_file, sample_pattern_file, capsys):
        main(["search", "--patterns", sample_pattern_file, sample_text_file])
        out = capsys.readouterr().out
        assert "ABAB" in out or "hello" in out


class TestCompareCommand:
    def test_compare_basic(self, sample_text_file, capsys):
        main(["compare", "ABAB", sample_text_file])
        out = capsys.readouterr().out
        assert "Algorithm" in out
        assert "kmp" in out

    def test_compare_specific_algos(self, sample_text_file, capsys):
        main(["compare", "ABAB", sample_text_file, "--algos", "naive,kmp"])
        out = capsys.readouterr().out
        assert "naive" in out
        assert "kmp" in out
