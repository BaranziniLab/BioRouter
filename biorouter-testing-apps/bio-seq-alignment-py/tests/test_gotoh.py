"""Tests for Gotoh affine gap alignment."""

import pytest
from bio_seq_align.align.gotoh import gotoh_align


class TestGotoh:
    # ── Basic correctness ────────────────────────────────────

    def test_identical_sequences(self):
        r = gotoh_align("ACDEFG", "ACDEFG")
        assert r.score > 0
        assert r.identity == pytest.approx(1.0)

    def test_no_gaps_needed(self):
        r = gotoh_align("ACGT", "ACGT")
        assert r.gaps == 0
        assert r.matches == 4

    def test_single_gap_open(self):
        """A single gap should cost gap_open + gap_extend."""
        r = gotoh_align("ACDEFG", "ACEFG")
        assert r.gaps > 0
        # Score should reflect gap penalty — lower than perfect all-match alignment
        perfect = gotoh_align("ACDEFG", "ACDEFG")
        assert r.score < perfect.score

    def test_affine_cheaper_for_long_gaps(self):
        """Affine gaps should score better than linear for long gaps."""
        # Two sequences needing a long gap: "ACDEFGHIKLM" (11) vs "ACDELM" (6)
        seq1 = "ACDEFGHIKLM"
        seq2 = "ACDELM"
        # Compare Gotoh (affine) with NW (linear) using comparable total cost
        from bio_seq_align.align.nw import needleman_wunsch
        r_affine = gotoh_align(seq1, seq2, gap_open=-5, gap_extend=-1)
        r_linear = needleman_wunsch(seq1, seq2, gap_penalty=-6)
        # Affine total for a 5-residue gap: -5 + 5*(-1) = -10
        # Linear total for a 5-residue gap at -6: 5*(-6) = -30
        assert r_affine.score > r_linear.score

    # ── Affine vs linear distinction ─────────────────────────

    def test_affine_gap_open_vs_extend(self):
        """Changing gap_open vs gap_extend should affect scores differently."""
        seq1 = "ACDEFGHIKLM"
        seq2 = "ACDELM"
        r1 = gotoh_align(seq1, seq2, gap_open=-5, gap_extend=-1)
        r2 = gotoh_align(seq1, seq2, gap_open=-10, gap_extend=-1)
        assert r1.score > r2.score  # more negative open → lower score

    # ── Symmetry ─────────────────────────────────────────────

    def test_score_symmetric(self):
        r1 = gotoh_align("ACDEFG", "ACEG")
        r2 = gotoh_align("ACEG", "ACDEFG")
        assert r1.score == r2.score

    # ── Edge cases ───────────────────────────────────────────

    def test_empty_seq1(self):
        r = gotoh_align("", "ACDEFG")
        assert len(r.aligned_seq1) == len(r.aligned_seq2)

    def test_empty_seq2(self):
        r = gotoh_align("ACDEFG", "")
        assert len(r.aligned_seq1) == len(r.aligned_seq2)

    def test_both_empty(self):
        r = gotoh_align("", "")
        assert r.score == 0

    def test_single_char(self):
        r = gotoh_align("A", "A")
        assert r.score > 0
        assert r.identity == 1.0

    # ── Local mode ───────────────────────────────────────────

    def test_local_mode(self):
        r = gotoh_align("XXACDEFGXX", "YYACDEFGYY", mode="local")
        assert r.score > 0
        assert r.identity == pytest.approx(1.0)

    def test_local_mode_subsequence(self):
        r = gotoh_align("ACDEFGHIKLM", "CDEF", mode="local")
        assert r.score > 0

    # ── Result structure ─────────────────────────────────────

    def test_result_fields(self):
        r = gotoh_align("ACDEFG", "ACDEFG")
        assert "Gotoh" in r.algorithm
        assert len(r.aligned_seq1) == len(r.aligned_seq2)
