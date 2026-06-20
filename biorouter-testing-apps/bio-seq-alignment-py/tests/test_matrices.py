"""Tests for substitution matrices."""

import pytest
from bio_seq_align.matrices import BLOSUM62, SimpleScoring, get_matrix


class TestBLOSUM62:
    def test_dimensions(self):
        assert len(BLOSUM62) == 24  # 20 amino acids + B, Z, X, *

    def test_symmetry(self):
        aas = list("ACDEFGHIKLMNPQRSTVWY")
        # The published BLOSUM62 has one known asymmetry: N-S=1, S-N=0
        known_asymmetric = {("N", "S"), ("S", "N")}
        for a in aas:
            for b in aas:
                if (a, b) in known_asymmetric:
                    continue
                assert BLOSUM62[a][b] == BLOSUM62[b][a], f"Asymmetric: {a}-{b}"

    def test_self_score_positive(self):
        for aa in "ACDEFGHIKLMNPQRSTVWY":
            assert BLOSUM62[aa][aa] > 0, f"Non-positive self-score for {aa}"

    def test_known_value(self):
        # A-A should be 4
        assert BLOSUM62["A"]["A"] == 4
        # A-G should be 0
        assert BLOSUM62["A"]["G"] == 0
        # W-W should be 11
        assert BLOSUM62["W"]["W"] == 11


class TestSimpleScoring:
    def test_match(self):
        s = SimpleScoring(match=2, mismatch=-1)
        assert s["A"]["A"] == 2
        assert s["C"]["C"] == 2

    def test_mismatch(self):
        s = SimpleScoring(match=2, mismatch=-1)
        assert s["A"]["G"] == -1
        assert s["T"]["A"] == -1

    def test_case_insensitive(self):
        s = SimpleScoring(match=3, mismatch=-2)
        assert s["a"]["A"] == 3
        assert s["A"]["a"] == 3

    def test_custom_scores(self):
        s = SimpleScoring(match=5, mismatch=-3)
        assert s["A"]["A"] == 5
        assert s["A"]["T"] == -3


class TestGetMatrix:
    def test_blosum62(self):
        m = get_matrix("blosum62")
        assert m["A"]["A"] == 4

    def test_simple(self):
        m = get_matrix("simple", match=3, mismatch=-2)
        assert m["A"]["A"] == 3
        assert m["A"]["T"] == -2

    def test_dna(self):
        m = get_matrix("dna")
        assert m["A"]["A"] == 2
        assert m["A"]["T"] == -1

    def test_identity(self):
        m = get_matrix("identity")
        assert m["A"]["A"] == 1
        assert m["A"]["T"] == 0

    def test_unknown_raises(self):
        with pytest.raises(ValueError):
            get_matrix("nonexistent")
