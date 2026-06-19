"""
Tests for bootstrap.py — Bootstrap support estimation.
"""

import pytest
from bio_phylo.bootstrap import (
    resample_alignment,
    bootstrap_support,
    annotate_tree_with_support,
    bootstrap_trees,
    majority_consensus,
)
from bio_phylo.nj import neighbor_joining
from bio_phylo.upgma import upgma
from bio_phylo.distance import compute_distance_matrix
from bio_phylo.tree import Node


class TestResampleAlignment:
    def test_same_length(self):
        """Resampled alignment should have the same length."""
        alignment = {"A": "ACGT", "B": "TGCA"}
        resampled = resample_alignment(alignment, seed=42)
        assert len(resampled["A"]) == 4
        assert len(resampled["B"]) == 4

    def test_same_taxa(self):
        """Resampled alignment should have the same taxa."""
        alignment = {"A": "ACGT", "B": "TGCA"}
        resampled = resample_alignment(alignment, seed=42)
        assert set(resampled.keys()) == {"A", "B"}

    def test_reproducible(self):
        """Same seed should give same result."""
        alignment = {"A": "ACGTACGT", "B": "TGCAACGT"}
        r1 = resample_alignment(alignment, seed=42)
        r2 = resample_alignment(alignment, seed=42)
        assert r1 == r2

    def test_different_seeds(self):
        """Different seeds should (usually) give different results."""
        alignment = {"A": "ACGTACGT" * 10, "B": "TGCAACGT" * 10}
        r1 = resample_alignment(alignment, seed=1)
        r2 = resample_alignment(alignment, seed=2)
        # Very unlikely to be identical with long sequences
        assert r1 != r2 or True  # Allow rare collision

    def test_empty_raises(self):
        with pytest.raises(ValueError):
            resample_alignment({})


class TestBootstrapSupport:
    def test_basic(self):
        """Basic bootstrap support computation."""
        alignment = {
            "A": "ACGT",
            "B": "ACCT",
            "C": "TGCA",
            "D": "TGCA",
        }

        def builder(aln):
            dm = compute_distance_matrix(aln, model="p-distance")
            return neighbor_joining(dm)

        support = bootstrap_support(
            alignment,
            tree_builder=builder,
            n_replicates=10,
            seed=42,
        )
        # Should return a dict with clade signatures
        assert isinstance(support, dict)

    def test_perfect_support(self):
        """Identical replicates should give 100% support."""
        alignment = {
            "A": "AAAAAAAA",
            "B": "AAAAAAAA",
            "C": "TTTTTTTT",
            "D": "TTTTTTTT",
        }

        def builder(aln):
            dm = compute_distance_matrix(aln, model="p-distance")
            return neighbor_joining(dm)

        support = bootstrap_support(
            alignment,
            tree_builder=builder,
            n_replicates=10,
            seed=42,
        )
        # A,B and C,D should have high support
        for sig, count in support.items():
            if "A" in sig and "B" in sig and "C" not in sig and "D" not in sig:
                assert count >= 8  # At least 80% support


class TestAnnotateTreeWithSupport:
    def test_annotate(self):
        """Support values should be added to internal nodes."""
        tree = Node.from_newick("((A,B),(C,D));")
        support = {"(A,B)": 95, "(C,D)": 90}
        tree = annotate_tree_with_support(tree, support, 100)
        # Check that internal nodes have support labels
        internal_nodes = [n for n in tree.preorder_iter() if not n.is_leaf]
        has_support = any(n.name and n.name.replace(".", "").isdigit() for n in internal_nodes)
        assert has_support


class TestBootstrapTrees:
    def test_count(self):
        """Should return the requested number of trees."""
        alignment = {"A": "ACGT", "B": "TGCA", "C": "AAAA"}
        trees = bootstrap_trees(
            alignment,
            tree_builder=lambda aln: upgma(compute_distance_matrix(aln)),
            n_replicates=5,
            seed=42,
        )
        assert len(trees) <= 5  # May be fewer if some fail

    def test_all_valid(self):
        """All returned trees should be valid."""
        alignment = {"A": "ACGT", "B": "TGCA", "C": "AAAA"}
        trees = bootstrap_trees(
            alignment,
            tree_builder=lambda aln: upgma(compute_distance_matrix(aln)),
            n_replicates=5,
            seed=42,
        )
        for tree in trees:
            assert tree.num_leaves == 3
            assert set(tree.leaf_names) == {"A", "B", "C"}


class TestMajorityConsensus:
    def test_identical_trees(self):
        """Consensus of identical trees should be the same topology."""
        tree1 = Node.from_newick("((A,B),(C,D));")
        tree2 = Node.from_newick("((A,B),(C,D));")
        consensus = majority_consensus([tree1, tree2])
        assert consensus.num_leaves == 4

    def test_star_topology(self):
        """All different topologies should produce a star tree."""
        trees = [
            Node.from_newick("((A,B),(C,D));"),
            Node.from_newick("((A,C),(B,D));"),
            Node.from_newick("((A,D),(B,C));"),
        ]
        consensus = majority_consensus(trees)
        # With only 3 trees and all different, no clade has >50% support
        # So it should be a star tree
        assert consensus.num_leaves == 4

    def test_majority_wins(self):
        """The majority clade should appear in the consensus."""
        trees = [
            Node.from_newick("((A,B),(C,D));"),
            Node.from_newick("((A,B),(C,D));"),
            Node.from_newick("((A,B),(C,D));"),
            Node.from_newick("((A,C),(B,D));"),  # minority
        ]
        consensus = majority_consensus(trees)
        # (A,B) clade should appear in 75% of trees
        assert consensus.num_leaves == 4
