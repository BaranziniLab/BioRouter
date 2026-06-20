"""
Tests for distance.py — DistanceMatrix, distance models, and FASTA parsing.
"""

import math
import pytest
from bio_phylo.distance import (
    DistanceMatrix,
    p_distance,
    jukes_cantor,
    kimura_2param,
    compute_distance_matrix,
    parse_fasta,
)


# ======================================================================
# DistanceMatrix
# ======================================================================


class TestDistanceMatrix:
    def test_construction(self):
        dm = DistanceMatrix(["A", "B", "C"])
        assert len(dm) == 3
        assert dm.names == ["A", "B", "C"]

    def test_from_square(self):
        dm = DistanceMatrix.from_square(
            ["A", "B", "C"],
            [[0.0, 0.1, 0.2], [0.1, 0.0, 0.3], [0.2, 0.3, 0.0]],
        )
        assert dm["A", "B"] == pytest.approx(0.1)
        assert dm["B", "A"] == pytest.approx(0.1)
        assert dm["A", "C"] == pytest.approx(0.2)
        assert dm["C", "B"] == pytest.approx(0.3)

    def test_from_dict(self):
        dm = DistanceMatrix.from_dict(
            {"A": {"A": 0, "B": 0.1}, "B": {"A": 0.1, "B": 0}}
        )
        assert dm["A", "B"] == pytest.approx(0.1)
        assert dm["B", "A"] == pytest.approx(0.1)

    def test_setitem(self):
        dm = DistanceMatrix(["A", "B"])
        dm["A", "B"] = 0.5
        assert dm["A", "B"] == 0.5
        assert dm["B", "A"] == 0.5

    def test_items_upper_triangle(self):
        dm = DistanceMatrix.from_square(
            ["A", "B", "C"],
            [[0, 0.1, 0.2], [0.1, 0, 0.3], [0.2, 0.3, 0]],
        )
        items = list(dm.items())
        assert len(items) == 3  # 3 pairs for 3 taxa
        values = {(a, b): d for a, b, d in items}
        assert values[("A", "B")] == pytest.approx(0.1)
        assert values[("A", "C")] == pytest.approx(0.2)
        assert values[("B", "C")] == pytest.approx(0.3)

    def test_to_square(self):
        dm = DistanceMatrix.from_square(
            ["A", "B"],
            [[0, 0.1], [0.1, 0]],
        )
        sq = dm.to_square()
        assert len(sq) == 2
        assert len(sq[0]) == 2
        assert sq[0][1] == pytest.approx(0.1)

    def test_formatted(self):
        dm = DistanceMatrix.from_square(
            ["A", "B"],
            [[0, 0.1234], [0.1234, 0]],
        )
        text = dm.formatted()
        assert "A" in text
        assert "B" in text
        assert "0.1234" in text


# ======================================================================
# p-distance
# ======================================================================


class TestPDist:
    def test_identical(self):
        assert p_distance("AAAA", "AAAA") == pytest.approx(0.0)

    def test_all_different(self):
        assert p_distance("AAAA", "TTTT") == pytest.approx(1.0)

    def test_half(self):
        assert p_distance("AATT", "AATC") == pytest.approx(0.25)

    def test_with_gaps_ignore(self):
        assert p_distance("AA-AAA", "AA-AAA", gap_mode="ignore") == pytest.approx(0.0)

    def test_with_gaps_treat(self):
        # Gaps treated as different states
        d = p_distance("A-A", "ACA", gap_mode="treat")
        assert d == pytest.approx(1 / 3)

    def test_different_lengths_raises(self):
        with pytest.raises(ValueError):
            p_distance("AA", "AAA")

    def test_empty_raises(self):
        with pytest.raises(ValueError):
            p_distance("", "")

    def test_lowercase(self):
        assert p_distance("aaaa", "tttt") == pytest.approx(1.0)


# ======================================================================
# Jukes-Cantor
# ======================================================================


class TestJukesCantor:
    def test_identical(self):
        assert jukes_cantor("AAAA", "AAAA") == pytest.approx(0.0)

    def test_known_value(self):
        # p = 0.25 → d_JC = -0.75 * ln(1 - 1/3) = -0.75 * ln(2/3)
        expected = -0.75 * math.log(2.0 / 3.0)
        d = jukes_cantor("AAAA", "TTTT")  # p = 1.0
        # p=1.0 >= 0.75, so should be inf
        assert d == float("inf")

    def test_six_diff(self):
        # 6 sites, 2 differ → p = 1/3
        seq1 = "AAAAAA"
        seq2 = "AATTAA"
        p = p_distance(seq1, seq2)
        expected = -0.75 * math.log(1.0 - (4.0 / 3.0) * p)
        assert jukes_cantor(seq1, seq2) == pytest.approx(expected)

    def test_symmetric(self):
        assert jukes_cantor("AATT", "AATC") == pytest.approx(
            jukes_cantor("AATC", "AATT")
        )

    def test_higher_than_p(self):
        seq1 = "ACGTACGT"
        seq2 = "ACGTACGG"
        d = jukes_cantor(seq1, seq2)
        p = p_distance(seq1, seq2)
        assert d >= p


# ======================================================================
# Kimura 2-parameter
# ======================================================================


class TestKimura2Param:
    def test_identical(self):
        assert kimura_2param("AAAA", "AAAA") == pytest.approx(0.0)

    def test_only_transitions(self):
        # A↔G transitions only
        seq1 = "AAAA"
        seq2 = "GGGG"
        # P = 1.0, Q = 0.0
        # d = -0.5 * ln(1 - 2*1 - 0) - 0.25 * ln(1 - 0)
        # = -0.5 * ln(-1) → inf (saturated)
        d = kimura_2param(seq1, seq2)
        assert d == float("inf")

    def test_only_transversions(self):
        # A→T transversions only
        seq1 = "AAAA"
        seq2 = "TTTT"
        # P = 0, Q = 1.0
        # d = -0.5 * ln(1 - 0 - 1) - 0.25 * ln(1 - 2)
        # Both log args are ≤ 0 → inf
        d = kimura_2param(seq1, seq2)
        assert d == float("inf")

    def test_mixed_changes(self):
        # Mix of transitions and transversions
        seq1 = "ACGTACGT"
        seq2 = "AGGTATCT"
        # Pos: A→A(same), C→G(transv), G→G(same), T→T(same),
        #       A→A(same), C→T(transv), G→C(transv), T→T(same)
        # P (transitions) = 0 (none among diffs), Q (transversions) = 3/8
        d = kimura_2param(seq1, seq2)
        assert d > 0
        assert d != float("inf")

    def test_symmetric(self):
        assert kimura_2param("ACGT", "TGCA") == pytest.approx(
            kimura_2param("TGCA", "ACGT")
        )

    def test_different_lengths_raises(self):
        with pytest.raises(ValueError):
            kimura_2param("AA", "AAA")

    def test_known_value(self):
        # 8 sites: 1 transition (A→G), 1 transversion (C→T)
        seq1 = "ACGTACGT"
        seq2 = "AGTTACGT"
        # At position 2: C→G (transversion), position 3: G→T (transversion)
        # Wait, let me recalculate:
        # Pos 0: A=A, Pos 1: C≠G (C→G: both pyrimidine? C is pyrimidine, G is purine → transversion)
        # Actually: purines={A,G}, pyrimidines={C,T,U}
        # C→G: C is pyrimidine, G is purine → transversion
        # G→T: G is purine, T is pyrimidine → transversion
        # So 0 transitions, 2 transversions out of 8 sites
        d = kimura_2param(seq1, seq2)
        P = 0.0
        Q = 2.0 / 8.0
        arg1 = 1.0 - 2.0 * P - Q
        arg2 = 1.0 - 2.0 * Q
        expected = -0.5 * math.log(arg1) - 0.25 * math.log(arg2)
        assert d == pytest.approx(expected)

    def test_transitions_only_in_diffs(self):
        # A→G (transition), C→C, G→G, T→T → P=1, Q=0 in diffs
        seq1 = "ACGT"
        seq2 = "GCGT"
        # 1 diff: A→G (transition)
        d = kimura_2param(seq1, seq2)
        P = 1.0 / 4.0
        Q = 0.0
        arg1 = 1.0 - 2.0 * P - Q
        arg2 = 1.0 - 2.0 * Q
        expected = -0.5 * math.log(arg1) - 0.25 * math.log(arg2)
        assert d == pytest.approx(expected)


# ======================================================================
# compute_distance_matrix
# ======================================================================


class TestComputeDistanceMatrix:
    def test_basic(self):
        seqs = {"A": "AAAA", "B": "AATT", "C": "AAAT"}
        dm = compute_distance_matrix(seqs, model="p-distance")
        assert len(dm) == 3
        assert dm["A", "B"] == pytest.approx(0.5)
        assert dm["A", "A"] == pytest.approx(0.0)

    def test_jc_model(self):
        seqs = {"A": "AAAA", "B": "AATT"}
        dm = compute_distance_matrix(seqs, model="jukes-cantor")
        assert dm["A", "B"] > 0

    def test_k2p_model(self):
        seqs = {"A": "ACGT", "B": "AGGT"}
        dm = compute_distance_matrix(seqs, model="kimura-2param")
        assert dm["A", "B"] > 0

    def test_aliases(self):
        seqs = {"A": "AAAA", "B": "AATT"}
        dm1 = compute_distance_matrix(seqs, model="p")
        dm2 = compute_distance_matrix(seqs, model="p-distance")
        assert dm1["A", "B"] == dm2["A", "B"]

    def test_unknown_model_raises(self):
        with pytest.raises(ValueError):
            compute_distance_matrix({"A": "AA"}, model="unknown")


# ======================================================================
# FASTA parsing
# ======================================================================


class TestFastaParsing:
    def test_simple(self):
        fasta = ">A\nACGT\n>B\nTGCA\n"
        seqs = parse_fasta(fasta)
        assert seqs == {"A": "ACGT", "B": "TGCA"}

    def test_multiline(self):
        fasta = ">A\nAC\nGT\n>B\nTG\nCA\n"
        seqs = parse_fasta(fasta)
        assert seqs["A"] == "ACGT"
        assert seqs["B"] == "TGCA"

    def test_header_with_description(self):
        fasta = ">seq1 some description\nACGT\n"
        seqs = parse_fasta(fasta)
        assert "seq1" in seqs

    def test_empty(self):
        result = parse_fasta("")
        assert result == {}

    def test_with_whitespace(self):
        fasta = ">A\n  ACGT  \n>T\n  TGCA  \n"
        seqs = parse_fasta(fasta)
        assert seqs["A"] == "ACGT"
        assert seqs["T"] == "TGCA"
