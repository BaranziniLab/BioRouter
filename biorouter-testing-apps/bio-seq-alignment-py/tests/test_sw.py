"""Tests for Smith-Waterman local alignment."""

import pytest
from bio_seq_align.align.sw import smith_waterman


class TestSmithWaterman:
    # ── Basic correctness ────────────────────────────────────

    def test_identical_sequences(self):
        r = smith_waterman("ACDEFG", "ACDEFG")
        assert r.score > 0
        assert r.identity == pytest.approx(1.0)

    def test_subsequence_match(self):
        """Should find the best local match."""
        r = smith_waterman("XXACDEFGXX", "YYACDEFGYY")
        assert r.score > 0
        assert r.identity == pytest.approx(1.0)

    def test_no_similarity(self):
        r = smith_waterman("AAAA", "TTTT")
        # With mismatch=-1, best local is 0 (empty alignment)
        assert r.score >= 0

    def test_partial_match(self):
        r = smith_waterman("ACDEFGHIKLM", "XXXCDEFXXX")
        assert r.score > 0
        assert r.identity > 0

    # ── Symmetry ─────────────────────────────────────────────

    def test_score_symmetric(self):
        r1 = smith_waterman("ACDEFG", "XXCDEX")
        r2 = smith_waterman("XXCDEX", "ACDEFG")
        assert r1.score == r2.score

    # ── Local property ───────────────────────────────────────

    def test_local_no_penalty_for_flanking(self):
        """Adding flanking characters shouldn't change the local score."""
        r1 = smith_waterman("ACDEFG", "CDEF")
        r2 = smith_waterman("XXACDEFGXX", "YYCDEFYY")
        assert r1.score == r2.score

    def test_score_nonnegative(self):
        """Smith-Waterman score is always >= 0."""
        r = smith_waterman("AAAA", "TTTT")
        assert r.score >= 0

    # ── Edge cases ───────────────────────────────────────────

    def test_empty_seq1(self):
        r = smith_waterman("", "ACDEFG")
        assert r.score == 0

    def test_empty_seq2(self):
        r = smith_waterman("ACDEFG", "")
        assert r.score == 0

    def test_both_empty(self):
        r = smith_waterman("", "")
        assert r.score == 0

    def test_single_char_match(self):
        r = smith_waterman("A", "A")
        assert r.score == 2
        assert r.identity == 1.0

    def test_single_char_mismatch(self):
        r = smith_waterman("A", "T")
        assert r.score == 0  # local: better to not align

    # ── Result structure ─────────────────────────────────────

    def test_result_fields(self):
        r = smith_waterman("ACDEFG", "CDEF")
        assert r.algorithm == "Smith-Waterman"
        assert r.matches + r.mismatches + r.gaps == r.length

    def test_aligned_lengths_equal(self):
        r = smith_waterman("ACDEFG", "XXCDEFXX")
        assert len(r.aligned_seq1) == len(r.aligned_seq2)
