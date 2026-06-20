"""Tests for progressive MSA."""

import pytest
from bio_seq_align.msa import progressive_msa, pairwise_distance_matrix, upgma


class TestPairwiseDistance:
    def test_self_distance_zero(self):
        seqs = ["ACDEFG", "HIKLMN"]
        dist = pairwise_distance_matrix(seqs)
        assert dist[0][0] == pytest.approx(0.0)
        assert dist[1][1] == pytest.approx(0.0)

    def test_symmetry(self):
        seqs = ["ACDEFG", "HIKLMN"]
        dist = pairwise_distance_matrix(seqs)
        assert dist[0][1] == pytest.approx(dist[1][0])

    def test_identical_sequences_zero(self):
        seqs = ["ACDEFG", "ACDEFG"]
        dist = pairwise_distance_matrix(seqs)
        assert dist[0][1] == pytest.approx(0.0)


class TestUPGMA:
    def test_two_leaves(self):
        dist = [[0.0, 0.5], [0.5, 0.0]]
        tree = upgma(dist, ["A", "B"])
        assert not tree.is_leaf
        assert set(tree.leaves()) == {"A", "B"}

    def test_three_leaves(self):
        dist = [
            [0.0, 0.2, 0.6],
            [0.2, 0.0, 0.5],
            [0.6, 0.5, 0.0],
        ]
        tree = upgma(dist, ["A", "B", "C"])
        assert set(tree.leaves()) == {"A", "B", "C"}


class TestProgressiveMSA:
    def test_two_sequences(self):
        seqs = ["ACDEFG", "ACDEFG"]
        result = progressive_msa(seqs)
        assert len(result) == 2
        assert len(result[0]) == len(result[1])

    def test_three_sequences(self):
        seqs = ["ACDEFG", "ACDEFG", "ACDEFG"]
        result = progressive_msa(seqs)
        assert len(result) == 3
        # All should be same length
        assert len(result[0]) == len(result[1]) == len(result[2])

    def test_aligned_length_consistent(self):
        """All output sequences must have the same length."""
        seqs = ["ACDEFG", "ACEG", "ACXXFG"]
        result = progressive_msa(seqs)
        lengths = [len(s) for s in result]
        assert len(set(lengths)) == 1

    def test_preserves_residues(self):
        """Gaps are added; original residues must be preserved."""
        seqs = ["ACDEFG", "ACEG"]
        result = progressive_msa(seqs)
        for orig, aligned in zip(seqs, result):
            assert aligned.replace("-", "") == orig

    def test_single_sequence(self):
        result = progressive_msa(["ACDEFG"])
        assert result == ["ACDEFG"]

    def test_labels(self):
        seqs = ["ACDEFG", "ACEG"]
        labels = ["human", "mouse"]
        result = progressive_msa(seqs, labels)
        assert len(result) == 2
