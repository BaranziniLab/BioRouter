"""
Bootstrap support estimation for phylogenetic trees.

Provides functions to:
- Resample columns from an alignment (non-parametric bootstrap)
- Build trees from bootstrap replicates
- Compute bootstrap support values for each branch in a reference tree
- Annotate a tree with support values
"""

from __future__ import annotations

import random
from collections import defaultdict
from typing import Callable, Optional

from bio_phylo.distance import DistanceMatrix, compute_distance_matrix
from bio_phylo.tree import Node


def resample_alignment(
    alignment: dict[str, str], seed: Optional[int] = None
) -> dict[str, str]:
    """Create a bootstrap replicate by sampling columns with replacement.

    Parameters
    ----------
    alignment : dict[str, str]
        {taxon_name: aligned_sequence}. All sequences must have the same length.
    seed : int, optional
        Random seed for reproducibility.

    Returns
    -------
    dict[str, str]
        Resampled alignment (same taxon names, same length, sampled columns).
    """
    if not alignment:
        raise ValueError("Empty alignment")

    names = list(alignment.keys())
    seq_len = len(alignment[names[0]])

    rng = random.Random(seed)
    indices = [rng.randint(0, seq_len - 1) for _ in range(seq_len)]

    resampled: dict[str, str] = {}
    for name in names:
        seq = alignment[name]
        resampled[name] = "".join(seq[i] for i in indices)
    return resampled


def _tree_topology_signature(tree: Node) -> str:
    """Create a canonical signature for a tree topology (ignoring branch lengths and labels).

    The signature encodes the nested structure of clades as a sorted tuple string.
    This allows comparing topologies across bootstrap replicates.
    """
    if tree.is_leaf:
        return tree.name

    child_sigs = sorted(_tree_topology_signature(c) for c in tree.children)
    return "(" + ",".join(child_sigs) + ")"


def _clade_signature(leaves: frozenset[str]) -> str:
    """Create a canonical signature for a clade (set of leaf names)."""
    return "(" + ",".join(sorted(leaves)) + ")"


def _get_clades(tree: Node) -> list[frozenset[str]]:
    """Get all clades (non-trivial subtrees) in a tree as sets of leaf names."""
    clades = []
    for node in tree.preorder_iter():
        if not node.is_leaf:
            leaves = frozenset(node.leaf_names)
            # Exclude the full set (root clade) — only internal clades
            if len(leaves) < tree.num_leaves and len(leaves) > 1:
                clades.append(leaves)
    return clades


def bootstrap_support(
    alignment: dict[str, str],
    tree_builder: Callable[[dict[str, str]], Node],
    n_replicates: int = 100,
    seed: Optional[int] = None,
    reference_tree: Optional[Node] = None,
) -> dict[str, int]:
    """Compute bootstrap support values for clades in a reference tree.

    Parameters
    ----------
    alignment : dict[str, str]
        Original alignment.
    tree_builder : callable
        Function that takes an alignment dict and returns a Node tree.
    n_replicates : int
        Number of bootstrap replicates.
    seed : int, optional
        Master random seed.
    reference_tree : Node, optional
        The tree to annotate. If None, the tree built from the original
        alignment is used as the reference.

    Returns
    -------
    dict[str, int]
        Mapping from clade signature → bootstrap count (out of n_replicates).
        Clades appearing in all replicates get n_replicates.
    """
    # Build reference tree if not provided
    if reference_tree is None:
        reference_tree = tree_builder(alignment)

    ref_clades = _get_clades(reference_tree)
    if not ref_clades:
        return {}

    # Count occurrences of each reference clade across replicates
    clade_counts: dict[str, int] = defaultdict(int)
    for clade in ref_clades:
        clade_counts[_clade_signature(clade)] = 0

    rng = random.Random(seed)
    for i in range(n_replicates):
        replicate_seed = rng.randint(0, 2**31 - 1)
        resampled = resample_alignment(alignment, seed=replicate_seed)
        try:
            rep_tree = tree_builder(resampled)
        except Exception:
            continue  # Skip failed replicates

        rep_clades = _get_clades(rep_tree)
        rep_clade_set = {_clade_signature(c) for c in rep_clades}

        for ref_clade in ref_clades:
            sig = _clade_signature(ref_clade)
            if sig in rep_clade_set:
                clade_counts[sig] += 1

    return dict(clade_counts)


def annotate_tree_with_support(
    tree: Node,
    support_counts: dict[str, int],
    n_replicates: int,
) -> Node:
    """Add bootstrap support values as internal node names/labels.

    For each internal node, sets ``node.name`` to the bootstrap percentage
    if the node's clade has a support count.

    Parameters
    ----------
    tree : Node
        The reference tree to annotate (modified in place).
    support_counts : dict[str, int]
        Output from ``bootstrap_support``.
    n_replicates : int
        Total number of replicates.

    Returns
    -------
    Node
        The same tree, annotated.
    """
    for node in tree.preorder_iter():
        if node.is_leaf or node.is_root:
            continue
        leaves = frozenset(node.leaf_names)
        sig = _clade_signature(leaves)
        if sig in support_counts:
            pct = support_counts[sig] / n_replicates * 100
            # Append support to existing name or replace
            if node.name and not node.name.startswith("("):
                node.name = f"{node.name}_{pct:.0f}"
            else:
                node.name = f"{pct:.0f}"
    return tree


def bootstrap_trees(
    alignment: dict[str, str],
    tree_builder: Callable[[dict[str, str]], Node],
    n_replicates: int = 100,
    seed: Optional[int] = None,
) -> list[Node]:
    """Generate bootstrap replicate trees.

    Parameters
    ----------
    alignment : dict[str, str}
        Original alignment.
    tree_builder : callable
        Function that takes an alignment dict and returns a Node tree.
    n_replicates : int
        Number of replicates to generate.
    seed : int, optional
        Random seed.

    Returns
    -------
    list[Node]
        List of trees from bootstrap replicates.
    """
    trees: list[Node] = []
    rng = random.Random(seed)
    for _ in range(n_replicates):
        rep_seed = rng.randint(0, 2**31 - 1)
        resampled = resample_alignment(alignment, seed=rep_seed)
        try:
            tree = tree_builder(resampled)
            trees.append(tree)
        except Exception:
            continue
    return trees


def majority_consensus(trees: list[Node]) -> Node:
    """Build a majority-rule consensus tree from a list of trees.

    Clades appearing in >50% of trees are included.
    """
    if not trees:
        raise ValueError("Empty tree list")

    clade_counts: dict[str, int] = defaultdict(int)
    total = len(trees)

    for tree in trees:
        for clade in _get_clades(tree):
            sig = _clade_signature(clade)
            clade_counts[sig] += 1

    # Keep clades with > 50% support
    consensus_clades = {sig for sig, count in clade_counts.items() if count > total / 2}

    if not consensus_clades:
        # Return a star tree
        leaves = trees[0].leaf_names
        root = Node(branch_length=0.0)
        for name in leaves:
            leaf = Node(name=name, branch_length=0.0)
            root.children.append(leaf)
            leaf.parent = root
        return root

    # Build consensus tree by nesting compatible clades
    # Parse all clade sets
    all_clade_sets: list[frozenset[str]] = []
    for sig in consensus_clades:
        # Parse "(A,B,C)" back to frozenset
        inner = sig[1:-1]  # remove parens
        if inner:
            all_clade_sets.append(frozenset(inner.split(",")))

    # Sort by size (largest first) for nesting
    all_clade_sets.sort(key=len, reverse=True)

    # Build the tree: start with all leaves, nest clades
    all_leaves = frozenset(trees[0].leaf_names)
    root = _build_consensus_tree(all_leaves, all_clade_sets)
    return root


def _build_consensus_tree(
    taxon_set: frozenset[str],
    clade_sets: list[frozenset[str]],
) -> Node:
    """Recursively build a consensus tree from compatible clades."""
    # Find clades that are proper subsets of taxon_set
    sub_clades = [c for c in clade_sets if c < taxon_set]

    if not sub_clades:
        # Star topology
        root = Node(branch_length=0.0)
        for name in sorted(taxon_set):
            leaf = Node(name=name, branch_length=0.0)
            root.children.append(leaf)
            leaf.parent = root
        return root

    # Find non-overlapping sub-clades
    groups: list[frozenset[str]] = []
    used = set()
    for clade in sub_clades:
        if not clade & used:
            groups.append(clade)
            used |= clade

    # Unassigned taxa
    unassigned = taxon_set - used

    # Build children
    root = Node(branch_length=0.0)
    remaining_clades = [c for c in clade_sets if not c < taxon_set]

    for group in groups:
        child = _build_consensus_tree(group, remaining_clades)
        root.children.append(child)
        child.parent = root

    if unassigned:
        for name in sorted(unassigned):
            leaf = Node(name=name, branch_length=0.0)
            root.children.append(leaf)
            leaf.parent = root

    return root
