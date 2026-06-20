"""
Tests for utils.py — Utilities for sequence I/O, validation, and matrix parsing.
"""

import os
import pytest
from bio_phylo.utils import (
    parse_fasta,
    read_fasta,
    write_fasta,
    parse_distance_matrix,
    validate_alignment,
    alignment_summary,
)


class TestParseFasta:
    def test_simple(self):
        fasta = ">A\nACGT\n>B\nTGCA\n"
        seqs = parse_fasta(fasta)
        assert seqs == {"A": "ACGT", "B": "TGCA"}

    def test_multiline(self):
        fasta = ">seq1\nAC\nGT\n>T2\nTG\nCA\n"
        seqs = parse_fasta(fasta)
        assert seqs["seq1"] == "ACGT"
        assert seqs["T2"] == "TGCA"

    def test_header_with_description(self):
        fasta = ">gene_1 some info\nACGT\n"
        seqs = parse_fasta(fasta)
        assert "gene_1" in seqs

    def test_empty(self):
        result = parse_fasta("")
        assert result == {}  # returns empty dict

    def test_whitespace_handling(self):
        fasta = ">A\n  ACGT  \n"
        seqs = parse_fasta(fasta)
        assert seqs["A"] == "ACGT"


class TestReadWriteFasta:
    def test_roundtrip(self, tmp_path):
        seqs = {"Human": "ATGC", "Mouse": "ATCC"}
        path = str(tmp_path / "test.fasta")
        write_fasta(seqs, path)
        loaded = read_fasta(path)
        assert loaded == seqs

    def test_wrapping(self, tmp_path):
        seqs = {"Seq1": "A" * 200}
        path = str(tmp_path / "long.fasta")
        write_fasta(seqs, path, wrap=80)
        with open(path) as f:
            lines = f.readlines()
        assert len(lines) > 2  # Header + multiple wrapped lines


class TestParseDistanceMatrix:
    def test_simple(self):
        text = """\
A B C
A 0.0 0.1 0.2
B 0.1 0.0 0.3
C 0.2 0.3 0.0
"""
        dm = parse_distance_matrix(text)
        assert len(dm) == 3
        assert dm["A", "B"] == pytest.approx(0.1)

    def test_empty_raises(self):
        with pytest.raises(ValueError):
            parse_distance_matrix("")


class TestValidateAlignment:
    def test_valid(self):
        seqs = {"A": "ACGT", "B": "TGCA"}
        issues = validate_alignment(seqs)
        assert issues == []

    def test_different_lengths(self):
        seqs = {"A": "ACGT", "B": "TG"}
        issues = validate_alignment(seqs)
        assert len(issues) > 0
        assert any("different lengths" in i for i in issues)

    def test_empty_sequence(self):
        seqs = {"A": "ACGT", "B": ""}
        issues = validate_alignment(seqs)
        assert len(issues) > 0

    def test_invalid_chars(self):
        seqs = {"A": "ACGT", "B": "TG12"}
        issues = validate_alignment(seqs)
        assert len(issues) > 0
        assert any("invalid" in i.lower() for i in issues)

    def test_empty_alignment(self):
        issues = validate_alignment({})
        assert len(issues) == 1
        assert "empty" in issues[0].lower()


class TestAlignmentSummary:
    def test_basic(self):
        seqs = {"A": "ACGT", "B": "TGCA"}
        summary = alignment_summary(seqs)
        assert "4 sequences" in summary or "2 sequences" in summary
        assert "positions" in summary

    def test_empty(self):
        assert "Empty" in alignment_summary({})
