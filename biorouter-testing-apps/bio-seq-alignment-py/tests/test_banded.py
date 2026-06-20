"""Tests for banded alignment."""

import pytest
from bio_seq_align.align.banded import banded_alignment
from bio_seq_align.align.nw import needleman_wunsch


class TestBandedAlignment:
    # ── Basic correctness ────────────────────────────────────

    def test_identical_sequences(self):
        r = banded_alignment("ACDEFG", "ACDEFG", bandwidth=3)
        assert r.score > 0
        assert r.identity == pytest.approx(1.0)

    def test_similar_to_unbanded(self):
        """With sufficient bandwidth, should match Needleman-Wunsch."""
        seq1 = "ACDEFG"
        seq2 = "ACEG"
        r_banded = banded_alignment(seq1, seq2, bandwidth=5)
        r_nw = needleman_wunsch(seq1, seq2)
        assert r_banded.score == r_nw.score

    def test_narrow_band_matches_wide(self):
        """For similar-length sequences, narrow band should still work."""
        seq1 = "ACDEFGHIKLM"
        seq2 = "ACDEFGHIKLM"
        r_narrow = banded_alignment(seq1, seq2, bandwidth=1)
        r_wide = banded_alignment(seq1, seq2, bandwidth=10)
        assert r_narrow.score == r_wide.score

    # ── Bandwidth effects ────────────────────────────────────

    def test_bandwidth_auto_widens(self):
        """If bandwidth < length diff, it should auto-widen."""
        seq1 = "ACDEFGHIKLM"
        seq2 = "AC"
        r = banded_alignment(seq1, seq2, bandwidth=1)
        r_nw = needleman_wunsch(seq1, seq2)
        # Should match NW since bandwidth was widened
        assert r.score == r_nw.score

    def test_wider_band_no_worse(self):
        """A wider band should produce score >= narrow band."""
        seq1 = "ACDEFGHIKLM"
        seq2 = "ACEGIKM"
        r_narrow = banded_alignment(seq1, seq2, bandwidth=2)
        r_wide = banded_alignment(seq1, seq2, bandwidth=5)
        assert r_wide.score >= r_narrow.score

    # ── Symmetry ─────────────────────────────────────────────

    def test_score_symmetric(self):
        r1 = banded_alignment("ACDEFG", "ACEG", bandwidth=5)
        r2 = banded_alignment("ACEG", "ACDEFG", bandwidth=5)
        assert r1.score == r2.score

    # ── Edge cases ───────────────────────────────────────────

    def test_empty_seq1(self):
        r = banded_alignment("", "ACDEFG", bandwidth=10)
        assert len(r.aligned_seq1) == len(r.aligned_seq2)

    def test_empty_seq2(self):
        r = banded_alignment("ACDEFG", "", bandwidth=10)
        assert len(r.aligned_seq1) == len(r.aligned_seq2)

    def test_both_empty(self):
        r = banded_alignment("", "", bandwidth=3)
        assert r.score == 0

    # ── Result structure ─────────────────────────────────────

    def test_result_fields(self):
        r = banded_alignment("ACDEFG", "ACDEFG", bandwidth=3)
        assert "Banded" in r.algorithm
        assert len(r.aligned_seq1) == len(r.aligned_seq2)
