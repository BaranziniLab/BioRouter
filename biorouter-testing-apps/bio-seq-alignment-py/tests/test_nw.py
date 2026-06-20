"""Tests for Needleman-Wunsch global alignment."""

import pytest
from bio_seq_align.align.nw import needleman_wunsch


class TestNeedlemanWunsch:
    # ── Basic correctness ────────────────────────────────────

    def test_identical_sequences(self):
        r = needleman_wunsch("ACDEFG", "ACDEFG")
        assert r.score > 0
        assert r.identity == pytest.approx(1.0)
        assert r.matches == 6
        assert r.gaps == 0

    def test_completely_different(self):
        r = needleman_wunsch("AAAA", "TTTT")
        assert r.identity < 1.0
        assert r.gaps == 0  # no reason to gap if all mismatches

    def test_known_alignment_simple(self):
        # Two sequences with a known best alignment
        r = needleman_wunsch("ACGT", "ACGT")
        assert r.aligned_seq1 == "ACGT"
        assert r.aligned_seq2 == "ACGT"
        assert r.score == 8  # 4 * match(2)

    def test_insertion(self):
        r = needleman_wunsch("ACDEFG", "ACDEFGHIKLM")
        assert len(r.aligned_seq1) == len(r.aligned_seq2)
        assert "-" in r.aligned_seq1  # gaps must appear
        assert r.aligned_seq1.replace("-", "") == "ACDEFG"
        assert r.aligned_seq2.replace("-", "") == "ACDEFGHIKLM"

    def test_deletion(self):
        r = needleman_wunsch("ACDEFGHIKLM", "ACDEFG")
        assert len(r.aligned_seq1) == len(r.aligned_seq2)
        assert "-" in r.aligned_seq2

    # ── Symmetry ─────────────────────────────────────────────

    def test_score_symmetric(self):
        """NW(A,B) and NW(B,A) should have the same score."""
        r1 = needleman_wunsch("ACDEFG", "ACEG")
        r2 = needleman_wunsch("ACEG", "ACDEFG")
        assert r1.score == r2.score

    def test_identity_symmetric(self):
        r1 = needleman_wunsch("ACDEFG", "ACEG")
        r2 = needleman_wunsch("ACEG", "ACDEFG")
        assert r1.identity == pytest.approx(r2.identity)

    # ── Gap penalty effects ──────────────────────────────────

    def test_more_gaps_with_higher_penalty(self):
        """A more negative gap penalty should produce fewer gaps in the alignment."""
        # Align sequences that need insertions; harsher penalty → more mismatches instead of gaps
        r1 = needleman_wunsch("ACDEFG", "ACEFG", gap_penalty=-1)
        r2 = needleman_wunsch("ACDEFG", "ACEFG", gap_penalty=-10)
        # With gap_penalty=-1 the aligner prefers to gap; with -10 it may mismatch instead
        assert r1.gaps >= r2.gaps
        assert r1.score >= r2.score

    def test_gap_penalty_changes_alignment(self):
        """With different gap penalties, the optimal alignment can change."""
        r1 = needleman_wunsch("ACDEFGHIKLM", "ACEGIKM", gap_penalty=-1)
        r2 = needleman_wunsch("ACDEFGHIKLM", "ACEGIKM", gap_penalty=-10)
        # The score difference should be significant
        assert r1.score > r2.score  # less penalty → higher score

    # ── Edge cases ───────────────────────────────────────────

    def test_empty_seq1(self):
        r = needleman_wunsch("", "ACDEFG")
        assert r.score == pytest.approx(-2 * 6)  # 6 gaps
        assert r.aligned_seq1 == "------"
        assert r.aligned_seq2 == "ACDEFG"
        assert r.identity == pytest.approx(0.0)

    def test_empty_seq2(self):
        r = needleman_wunsch("ACDEFG", "")
        assert r.score == pytest.approx(-2 * 6)
        assert r.aligned_seq2 == "------"
        assert r.aligned_seq1 == "ACDEFG"

    def test_both_empty(self):
        r = needleman_wunsch("", "")
        assert r.score == 0
        assert r.aligned_seq1 == ""
        assert r.aligned_seq2 == ""
        assert r.identity == 0.0

    def test_single_char_match(self):
        r = needleman_wunsch("A", "A")
        assert r.score == 2
        assert r.identity == 1.0

    def test_single_char_mismatch(self):
        r = needleman_wunsch("A", "T")
        assert r.score == -1  # mismatch score

    # ── DNA vs protein detection ─────────────────────────────

    def test_dna_auto_detection(self):
        r = needleman_wunsch("ACGTACGT", "ACGTACGT")
        assert r.score == 16  # 8 * match(2)

    def test_protein_auto_detection(self):
        r = needleman_wunsch("ACDEFG", "ACDEFG")
        assert r.score > 0
        assert r.identity == 1.0

    # ── Result structure ─────────────────────────────────────

    def test_result_fields(self):
        r = needleman_wunsch("ACGT", "ACGT")
        assert r.algorithm == "Needleman-Wunsch"
        assert r.matches + r.mismatches + r.gaps == r.length
        assert r.start1 == 0
        assert r.end1 == 4
        assert r.start2 == 0
        assert r.end2 == 4

    def test_aligned_lengths_equal(self):
        r = needleman_wunsch("ACDEFG", "ACEG")
        assert len(r.aligned_seq1) == len(r.aligned_seq2)
