"""Tests for CLI."""

import pytest
import sys
from io import StringIO
from unittest.mock import patch

from bio_seq_align.cli import main


class TestCLI:
    def test_basic_alignment(self, capsys):
        main(["--seq1", "ACDEFG", "--seq2", "ACDEFG", "--no-color"])
        out = capsys.readouterr().out
        assert "Needleman-Wunsch" in out
        assert "100.0%" in out

    def test_smith_waterman(self, capsys):
        main(["--seq1", "ACDEFG", "--seq2", "CDEF", "--algo", "sw", "--no-color"])
        out = capsys.readouterr().out
        assert "Smith-Waterman" in out

    def test_gotoh(self, capsys):
        main(["--seq1", "ACDEFG", "--seq2", "ACEG", "--algo", "gotoh", "--no-color"])
        out = capsys.readouterr().out
        assert "Gotoh" in out

    def test_banded(self, capsys):
        main(["--seq1", "ACDEFG", "--seq2", "ACDEFG", "--algo", "banded", "--no-color"])
        out = capsys.readouterr().out
        assert "Banded" in out

    def test_semi_global(self, capsys):
        main(["--seq1", "ACDEFG", "--seq2", "CDEF", "--algo", "semi-global", "--no-color"])
        out = capsys.readouterr().out
        assert "Semi-global" in out

    def test_overlap(self, capsys):
        main(["--seq1", "ABCDEF", "--seq2", "DEFXYZ", "--algo", "overlap", "--no-color"])
        out = capsys.readouterr().out
        assert "Semi-global" in out

    def test_fasta_input(self, tmp_path, capsys):
        fasta = tmp_path / "test.fasta"
        fasta.write_text(">seq1\nACDEFG\n>seq2\nACEG\n")
        main(["--fasta", str(fasta), "--no-color"])
        out = capsys.readouterr().out
        assert "Needleman-Wunsch" in out

    def test_msa_mode(self, capsys):
        main(["--seq1", "ACDEFG", "--seq2", "ACEG", "--algo", "msa"])
        out = capsys.readouterr().out
        assert "Progressive MSA" in out

    def test_custom_gap_penalty(self, capsys):
        main(["--seq1", "ACDEFG", "--seq2", "ACEG", "--gap", "-5", "--no-color"])
        out = capsys.readouterr().out
        assert "Needleman-Wunsch" in out

    def test_custom_bandwidth(self, capsys):
        main(["--seq1", "ACDEFG", "--seq2", "ACDEFG", "--algo", "banded", "--bandwidth", "1", "--no-color"])
        out = capsys.readouterr().out
        assert "Banded" in out
