"""
Tests for tree.py — Newick parsing, serialization, traversals, and operations.
"""

import pytest
from bio_phylo.tree import Node, from_newick, from_leaf_names, _path_length


# ======================================================================
# Node basics
# ======================================================================


class TestNodeBasics:
    def test_leaf_node(self):
        n = Node(name="A", branch_length=0.1)
        assert n.is_leaf
        assert n.is_root  # standalone node with no parent is root
        assert n.num_leaves == 1
        assert n.children == []

    def test_internal_node(self):
        c1 = Node(name="A")
        c2 = Node(name="B")
        parent = Node(children=[c1, c2])
        assert not parent.is_leaf
        assert parent.is_root
        assert parent.num_leaves == 2
        assert c1.parent is parent
        assert c2.parent is parent

    def test_depth_leaf(self):
        assert Node(name="A").depth == 0

    def test_depth_tree(self):
        # ((A,B),C)
        ab = Node(children=[Node("A"), Node("B")])
        root = Node(children=[ab, Node("C")])
        assert ab.depth == 1
        assert root.depth == 2

    def test_total_branch_length(self):
        a = Node(name="A", branch_length=0.1)
        b = Node(name="B", branch_length=0.2)
        parent = Node(branch_length=0.3, children=[a, b])
        assert parent.total_branch_length == pytest.approx(0.6)

    def test_leaves(self):
        a = Node(name="A")
        b = Node(name="B")
        c = Node(name="C")
        ab = Node(children=[a, b])
        root = Node(children=[ab, c])
        leaves = root.leaves
        assert len(leaves) == 3
        assert set(n.name for n in leaves) == {"A", "B", "C"}

    def test_leaf_names(self):
        a = Node(name="A")
        b = Node(name="B")
        root = Node(children=[a, b])
        assert root.leaf_names == ["A", "B"]

    def test_all_nodes_count(self):
        a = Node(name="A")
        b = Node(name="B")
        root = Node(children=[a, b])
        assert len(root.all_nodes) == 3  # root + 2 leaves


# ======================================================================
# Traversals
# ======================================================================


class TestTraversals:
    def _make_tree(self):
        # ((A,B),(C,D))
        a, b = Node("A"), Node("B")
        c, d = Node("C"), Node("D")
        ab = Node(children=[a, b])
        cd = Node(children=[c, d])
        root = Node(children=[ab, cd])
        return root

    def test_preorder(self):
        root = self._make_tree()
        names = [n.name for n in root.preorder_iter()]
        assert names[0] == ""  # root
        assert set(names) == {"", "A", "B", "C", "D"}

    def test_postorder(self):
        root = self._make_tree()
        names = [n.name for n in root.postorder_iter()]
        # Leaves should come before internal nodes
        leaf_idx = {n: i for i, n in enumerate(names) if n in ("A", "B", "C", "D")}
        internal_idx = {n: i for i, n in enumerate(names) if n == ""}
        # All leaves before root
        assert all(leaf_idx[n] < internal_idx[""] for n in leaf_idx)

    def test_levelorder(self):
        root = self._make_tree()
        names = [n.name for n in root.levelorder_iter()]
        assert names[0] == ""  # root first

    def test_leaf_iter(self):
        root = self._make_tree()
        leaf_names = [n.name for n in root.leaf_iter()]
        assert set(leaf_names) == {"A", "B", "C", "D"}


# ======================================================================
# Newick parsing and serialization
# ======================================================================


class TestNewick:
    def test_simple_leaf(self):
        tree = from_newick("A;")
        assert tree.is_leaf
        assert tree.name == "A"

    def test_simple_binary(self):
        tree = from_newick("(A,B);")
        assert tree.num_leaves == 2
        assert tree.leaf_names == ["A", "B"]

    def test_nested(self):
        tree = from_newick("((A,B),C);")
        assert tree.num_leaves == 3
        assert tree.is_binary()

    def test_with_branch_lengths(self):
        tree = from_newick("(A:0.1,B:0.2):0.3;")
        assert tree.branch_length == pytest.approx(0.3)
        # Check children
        children = tree.children
        bls = {c.name: c.branch_length for c in children}
        assert bls["A"] == pytest.approx(0.1)
        assert bls["B"] == pytest.approx(0.2)

    def test_complex_tree(self):
        tree = from_newick("((A:0.1,B:0.2):0.3,(C:0.4,D:0.5):0.6);")
        assert tree.num_leaves == 4
        assert tree.is_binary()

    def test_round_trip(self):
        original = "((A:0.100000,B:0.200000):0.300000,(C:0.400000,D:0.500000):0.600000);"
        tree = from_newick(original)
        output = tree.to_newick(precision=6)
        assert output == original

    def test_round_trip_simple(self):
        tree = from_newick("(A,B);")
        output = tree.to_newick()
        tree2 = from_newick(output)
        assert tree2.leaf_names == ["A", "B"]

    def test_empty_string_raises(self):
        with pytest.raises(ValueError):
            from_newick("")

    def test_semicolon_optional(self):
        tree = from_newick("(A,B)")
        assert tree.num_leaves == 2

    def test_quoted_names(self):
        tree = from_newick("('Taxon A','Taxon B');")
        names = tree.leaf_names
        assert "Taxon A" in names
        assert "Taxon B" in names

    def test_internal_labels(self):
        tree = from_newick("(A,B)internal;")
        assert tree.name == "internal"

    def test_deep_nesting(self):
        tree = from_newick("(((A,B),(C,D)),E);")
        assert tree.num_leaves == 5


# ======================================================================
# Tree operations
# ======================================================================


class TestTreeOperations:
    def test_num_internal_nodes(self):
        tree = from_newick("((A,B),(C,D));")
        assert tree.num_internal_nodes() == 3  # root + 2 internal

    def test_is_binary(self):
        tree = from_newick("((A,B),(C,D));")
        assert tree.is_binary()

    def test_is_not_binary(self):
        # Trifurcation
        tree = from_newick("(A,B,C);")
        assert not tree.is_binary()

    def test_height(self):
        tree = from_newick("(A:1.0,B:1.0):0.0;")
        assert tree.height() == pytest.approx(1.0)

    def test_height_asymmetric(self):
        tree = from_newick("(A:1.0,B:2.0):0.0;")
        assert tree.height() == pytest.approx(2.0)

    def test_get_clade(self):
        tree = from_newick("((A,B),(C,D));")
        clade = tree.get_clade({"A", "B"})
        assert set(clade.leaf_names) == {"A", "B"}

    def test_get_clade_whole_tree(self):
        tree = from_newick("((A,B),(C,D));")
        clade = tree.get_clade({"A", "B", "C", "D"})
        assert clade is tree

    def test_get_mrca(self):
        tree = from_newick("((A,B),(C,D));")
        leaves = {n.name: n for n in tree.leaf_iter()}
        mrca = tree.get_mrca(leaves["A"], leaves["B"])
        assert set(mrca.leaf_names) == {"A", "B"}

    def test_get_mrca_deeper(self):
        tree = from_newick("((A,B),(C,D));")
        leaves = {n.name: n for n in tree.leaf_iter()}
        mrca = tree.get_mrca(leaves["A"], leaves["D"])
        assert set(mrca.leaf_names) == {"A", "B", "C", "D"}

    def test_copy(self):
        tree = from_newick("((A:0.1,B:0.2):0.3,C:0.4);")
        copy = tree.copy()
        assert copy.leaf_names == tree.leaf_names
        # Modifying copy shouldn't affect original
        copy.name = "modified"
        assert tree.name == ""


# ======================================================================
# Rooting
# ======================================================================


class TestRooting:
    def test_root_at_internal(self):
        """Root at the MRCA of A and B (an internal node)."""
        tree = from_newick("((A,B),C);")
        leaves = {n.name: n for n in tree.leaf_iter()}
        # Find the AB internal node (MRCA of A and B)
        ab_node = tree.get_mrca(leaves["A"], leaves["B"])
        new_root = tree.root_at(ab_node)
        assert new_root is ab_node
        # After rerooting, all leaves should still be present
        all_leaves = set()
        for node in new_root.preorder_iter():
            if node.is_leaf:
                all_leaves.add(node.name)
        assert all_leaves == {"A", "B", "C"}


# ======================================================================
# from_leaf_names
# ======================================================================


class TestFromLeafNames:
    def test_basic(self):
        tree = from_leaf_names(["A", "B", "C"])
        assert tree.num_leaves == 3
        assert set(tree.leaf_names) == {"A", "B", "C"}


# ======================================================================
# Path length helper
# ======================================================================


class TestPathLength:
    def test_sibling_distance(self):
        a = Node("A", branch_length=1.0)
        b = Node("B", branch_length=2.0)
        root = Node(branch_length=0.0, children=[a, b])
        d = _path_length(a, b)
        assert d == pytest.approx(3.0)

    def test_parent_child_distance(self):
        a = Node("A", branch_length=1.0)
        root = Node(branch_length=0.0, children=[a])
        d = _path_length(a, root)
        assert d == pytest.approx(1.0)
