"""
Tests for upgma.py — UPGMA tree construction.
"""

import pytest
from bio_phylo.distance import DistanceMatrix
from bio_phylo.upgma import upgma
from bio_phylo.tree import Node


class TestUPGMA:
    def test_simple_3_taxa(self):
        """Classic 3-taxon UPGMA example."""
        dm = DistanceMatrix.from_square(
            ["A", "B", "C"],
            [[0, 2, 4], [2, 0, 4], [4, 4, 0]],
        )
        tree = upgma(dm)
        assert tree.num_leaves == 3
        leaves = tree.leaf_names
        assert set(leaves) == {"A", "B", "C"}

    def test_4_taxa(self):
        """UPGMA on 4 taxa."""
        dm = DistanceMatrix.from_square(
            ["A", "B", "C", "D"],
            [
                [0, 5, 9, 9],
                [5, 0, 10, 10],
                [9, 10, 0, 8],
                [9, 10, 8, 0],
            ],
        )
        tree = upgma(dm)
        assert tree.num_leaves == 4
        assert tree.is_binary()

    def test_ultrametric(self):
        """UPGMA should produce an ultrametric tree."""
        dm = DistanceMatrix.from_square(
            ["A", "B", "C", "D"],
            [
                [0, 5, 9, 9],
                [5, 0, 10, 10],
                [9, 10, 0, 8],
                [9, 10, 8, 0],
            ],
        )
        tree = upgma(dm)
        heights = []
        for leaf in tree.leaf_iter():
            h = 0.0
            node = leaf
            while node.parent is not None:
                h += node.branch_length or 0.0
                node = node.parent
            heights.append(h)

        for h in heights:
            assert h == pytest.approx(heights[0], rel=1e-6)

    def test_2_taxa(self):
        """Edge case: only 2 taxa."""
        dm = DistanceMatrix.from_square(
            ["A", "B"],
            [[0, 10], [10, 0]],
        )
        tree = upgma(dm)
        assert tree.num_leaves == 2
        assert tree.is_binary()

    def test_known_branch_lengths(self):
        """Verify UPGMA produces correct topology and ultrametric property."""
        dm = DistanceMatrix.from_square(
            ["A", "B", "C", "D"],
            [
                [0, 5, 9, 9],
                [5, 0, 10, 10],
                [9, 10, 0, 8],
                [9, 10, 8, 0],
            ],
        )
        tree = upgma(dm)
        # UPGMA root height = d(last_pair) / 2 = 9.5 / 2 = 4.75
        total = tree.height()
        assert total == pytest.approx(4.75, abs=0.1)

    def test_single_taxon(self):
        """Edge case: single taxon."""
        dm = DistanceMatrix.from_square(["A"], [[0]])
        tree = upgma(dm)
        assert tree.num_leaves == 1

    def test_tree_is_rooted(self):
        dm = DistanceMatrix.from_square(
            ["A", "B", "C"],
            [[0, 1, 2], [1, 0, 2], [2, 2, 0]],
        )
        tree = upgma(dm)
        assert tree.is_root

    def test_newick_round_trip(self):
        """UPGMA tree can be serialized and parsed back."""
        dm = DistanceMatrix.from_square(
            ["A", "B", "C", "D"],
            [
                [0, 5, 9, 9],
                [5, 0, 10, 10],
                [9, 10, 0, 8],
                [9, 10, 8, 0],
            ],
        )
        tree = upgma(dm)
        newick = tree.to_newick(precision=4)
        tree2 = Node.from_newick(newick)
        assert set(tree2.leaf_names) == set(tree.leaf_names)
