"""Tests for semi-global and overlap alignment."""

import pytest
from bio_seq_align.align.semi_global import semi_global_alignment, overlap_alignment
from bio_seq_align.align.nw import needleman_wunsch


class TestSemiGlobal:
    # ── Basic correctness ────────────────────────────────────

    def test_identical_sequences(self):
        r = semi_global_alignment("ACDEFG", "ACDEFG")
        assert r.score > 0
        assert r.identity == pytest.approx(1.0)

    def test_free_ends_higher_score(self):
        """Semi-global should score >= NW for sequences with different flanking."""
        seq1 = "XXACDEFG"
        seq2 = "ACDEFGYY"
        r_sg = semi_global_alignment(seq1, seq2)
        r_nw = needleman_wunsch(seq1, seq2)
        assert r_sg.score >= r_nw.score

    def test_substring_embedded(self):
        """Should find the best overlap even with flanking noise."""
        r = semi_global_alignment("XXACDEFGXX", "ACDEFG")
        assert r.score > 0
        assert r.identity > 0

    # ── Symmetry ─────────────────────────────────────────────

    def test_score_symmetric(self):
        r1 = semi_global_alignment("ACDEFG", "CDE")
        r2 = semi_global_alignment("CDE", "ACDEFG")
        assert r1.score == r2.score

    # ── Edge cases ───────────────────────────────────────────

    def test_empty_seq1(self):
        r = semi_global_alignment("", "ACDEFG")
        assert r.score >= 0

    def test_empty_seq2(self):
        r = semi_global_alignment("ACDEFG", "")
        assert r.score >= 0

    def test_both_empty(self):
        r = semi_global_alignment("", "")
        assert r.score == 0


class TestOverlap:
    def test_identical_sequences(self):
        r = overlap_alignment("ACDEFG", "ACDEFG")
        assert r.score > 0

    def test_suffix_prefix_overlap(self):
        """Should find overlap between suffix of seq1 and prefix of seq2."""
        r = overlap_alignment("ABCDEF", "DEFXYZ")
        assert r.score > 0
        assert r.identity > 0

    def test_no_overlap(self):
        """With no shared characters, score should be at most mismatched."""
        r = overlap_alignment("AAAA", "TTTT")
        # Overlap must align at least some positions, so score < 0 for all mismatches
        assert r.score < 0

    def test_result_fields(self):
        r = overlap_alignment("ACDEFG", "CDEF")
        assert r.algorithm == "Semi-global"
        assert len(r.aligned_seq1) == len(r.aligned_seq2)
