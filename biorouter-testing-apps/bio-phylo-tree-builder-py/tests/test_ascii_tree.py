"""
Tests for ascii_tree.py — ASCII tree rendering.
"""

import pytest
from bio_phylo.ascii_tree import ascii_tree, render_tree_compact, draw_tree_ascii
from bio_phylo.tree import from_newick


class TestAsciiTree:
    def test_simple_tree(self):
        tree = from_newick("(A,B);")
        output = ascii_tree(tree)
        assert "A" in output
        assert "B" in output
        assert isinstance(output, str)

    def test_with_branch_lengths(self):
        tree = from_newick("(A:0.1,B:0.2):0.3;")
        output = ascii_tree(tree, show_branch_lengths=True)
        assert "0.1" in output
        assert "0.2" in output

    def test_without_branch_lengths(self):
        tree = from_newick("(A:0.1,B:0.2):0.3;")
        output = ascii_tree(tree, show_branch_lengths=False)
        assert "A" in output
        assert "B" in output

    def test_nested_tree(self):
        tree = from_newick("((A,B),(C,D));")
        output = ascii_tree(tree)
        for name in ["A", "B", "C", "D"]:
            assert name in output

    def test_leaf_only(self):
        tree = from_newick("A;")
        output = ascii_tree(tree)
        assert "A" in output


class TestRenderCompact:
    def test_simple(self):
        tree = from_newick("((A:0.1,B:0.2):0.3,C:0.4);")
        output = render_tree_compact(tree, show_branch_lengths=True)
        assert "A" in output
        assert "B" in output
        assert "C" in output
        assert isinstance(output, str)

    def test_nested(self):
        tree = from_newick("(((A,B),C),D);")
        output = render_tree_compact(tree)
        for name in ["A", "B", "C", "D"]:
            assert name in output


class TestDrawTreeAscii:
    def test_proportional(self):
        tree = from_newick("(A:1.0,B:2.0);")
        output = draw_tree_ascii(tree, width=60)
        assert "A" in output
        assert "B" in output
        assert isinstance(output, str)
