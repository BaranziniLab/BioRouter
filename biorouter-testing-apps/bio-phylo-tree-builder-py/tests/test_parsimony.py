"""
Tests for parsimony.py — Fitch parsimony scoring and tree building.
"""

import pytest
from bio_phylo.parsimony import fitch_score, parsimony_greedy
from bio_phylo.tree import Node


class TestFitchScore:
    def test_identical_sequences(self):
        """Identical sequences should have score 0."""
        tree = Node(
            children=[
                Node(name="A", branch_length=0.0),
                Node(name="B", branch_length=0.0),
            ]
        )
        alignment = {"A": "ACGT", "B": "ACGT"}
        assert fitch_score(tree, alignment) == 0

    def test_single_difference(self):
        """One site differs → score 1."""
        tree = Node(
            children=[
                Node(name="A", branch_length=0.0),
                Node(name="B", branch_length=0.0),
            ]
        )
        alignment = {"A": "ACGT", "B": "ATGT"}
        assert fitch_score(tree, alignment) == 1

    def test_three_taxa(self):
        """Fitch score on a 3-taxon tree."""
        # ((A,B),C)
        ab = Node(children=[Node("A"), Node("B")])
        root = Node(children=[ab, Node("C")])
        alignment = {"A": "ACGT", "B": "ACCT", "C": "ACGT"}
        # Pos 0: A=A=C → 0
        # Pos 1: A=C=C → 0
        # Pos 2: C≠C=C → A and B differ (C vs C), C has C → 0
        # Wait: A=ACGT, B=ACCT, C=ACGT
        # Pos 0: A=A=A → 0
        # Pos 1: C=C=C → 0
        # Pos 2: G≠C=G → change between A/B, C matches A → 1
        # Pos 3: T=T=T → 0
        score = fitch_score(root, alignment)
        assert score == 1

    def test_all_different(self):
        """All different at one position → minimum 1 change."""
        tree = Node(
            children=[
                Node(name="A"),
                Node(name="B"),
            ]
        )
        alignment = {"A": "A", "B": "T"}
        assert fitch_score(tree, alignment) == 1

    def test_gap_handling(self):
        """Gaps should be handled (treated as unknown)."""
        tree = Node(
            children=[
                Node(name="A"),
                Node(name="B"),
            ]
        )
        alignment = {"A": "-", "B": "A"}
        score = fitch_score(tree, alignment)
        # Gap is unknown → no forced change
        assert score == 0

    def test_symmetric(self):
        """Score should be the same regardless of tree topology for 2 taxa."""
        tree = Node(children=[Node("A"), Node("B")])
        alignment = {"A": "ACGT", "B": "TGCA"}
        score = fitch_score(tree, alignment)
        assert score == 4  # All 4 positions differ

    def test_larger_alignment(self):
        """Fitch score on a larger alignment."""
        # Tree: ((A,B),(C,D))
        ab = Node(children=[Node("A"), Node("B")])
        cd = Node(children=[Node("C"), Node("D")])
        root = Node(children=[ab, cd])
        alignment = {
            "A": "ACGTACGT",
            "B": "ACGTACGT",
            "C": "TGCAACGT",
            "D": "TGCAACGT",
        }
        # Positions 0-3 differ between groups (4 changes), 4-7 identical (0 changes)
        score = fitch_score(root, alignment)
        assert score == 4


class TestParsimonyGreedy:
    def test_3_taxa(self):
        """Greedy parsimony on 3 taxa."""
        alignment = {"A": "ACGT", "B": "ACCT", "C": "ACGT"}
        tree = parsimony_greedy(alignment)
        assert tree.num_leaves == 3

    def test_4_taxa(self):
        """Greedy parsimony on 4 taxa."""
        alignment = {
            "A": "ACGT",
            "B": "ACCT",
            "C": "TGCA",
            "D": "TGCA",
        }
        tree = parsimony_greedy(alignment)
        assert tree.num_leaves == 4
        # A and B should be grouped (similar)
        # C and D should be grouped (identical)

    def test_minimal_score(self):
        """The greedy tree should have a reasonable (not worst) score."""
        alignment = {
            "A": "ACGT",
            "B": "ACCT",
            "C": "TGCA",
            "D": "TGCA",
        }
        tree = parsimony_greedy(alignment)
        score = fitch_score(tree, alignment)
        # The optimal score for this alignment should be small
        # A vs B: 1 diff (pos 2), C vs D: 0 diff, groups differ: 3 sites
        # Optimal: ((A,B),(C,D)) with score = 1 + 3 = 4
        assert score <= 6  # Should be near optimal

    def test_2_taxa(self):
        """Edge case: 2 taxa."""
        alignment = {"A": "ACGT", "B": "TGCA"}
        tree = parsimony_greedy(alignment)
        assert tree.num_leaves == 2
        score = fitch_score(tree, alignment)
        assert score == 4
