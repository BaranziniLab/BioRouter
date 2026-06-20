"""Tests for FASTA parser."""

import pytest
from pathlib import Path
import tempfile

from bio_seq_align.fasta import parse_fasta, read_fasta, write_fasta, FastaRecord


class TestParseFasta:
    def test_single_record(self):
        text = ">seq1\nACDEFG\n"
        records = parse_fasta(text)
        assert len(records) == 1
        assert records[0].id == "seq1"
        assert records[0].sequence == "ACDEFG"

    def test_multi_record(self):
        text = ">seq1\nACDEFG\n>seq2\nHIKLMN\n"
        records = parse_fasta(text)
        assert len(records) == 2
        assert records[0].id == "seq1"
        assert records[1].id == "seq2"

    def test_multiline_sequence(self):
        text = ">seq1\nACDE\nFGHI\nKLMN\n"
        records = parse_fasta(text)
        assert records[0].sequence == "ACDEFGHIKLMN"

    def test_description(self):
        text = ">seq1 some description here\nACDEFG\n"
        records = parse_fasta(text)
        assert records[0].id == "seq1"
        assert records[0].description == "some description here"

    def test_empty(self):
        records = parse_fasta("")
        assert records == []

    def test_whitespace_sequence(self):
        text = ">seq1\nAC DE FG\n"
        records = parse_fasta(text)
        assert records[0].sequence == "ACDEFG"

    def test_lowercase(self):
        text = ">seq1\nacdefg\n"
        records = parse_fasta(text)
        assert records[0].sequence == "ACDEFG"


class TestReadWriteFasta:
    def test_roundtrip(self, tmp_path):
        records = [
            FastaRecord("seq1", "test seq", "ACDEFG"),
            FastaRecord("seq2", "", "HIKLMN"),
        ]
        path = tmp_path / "test.fasta"
        write_fasta(records, path)
        loaded = read_fasta(path)
        assert len(loaded) == 2
        assert loaded[0].id == "seq1"
        assert loaded[0].sequence == "ACDEFG"
        assert loaded[1].id == "seq2"


class TestFastaRecord:
    def test_len(self):
        r = FastaRecord("x", "", "ACDEFG")
        assert len(r) == 6

    def test_str(self):
        r = FastaRecord("x", "desc", "ACD")
        s = str(r)
        assert s.startswith(">x desc")
        assert "ACD" in s
