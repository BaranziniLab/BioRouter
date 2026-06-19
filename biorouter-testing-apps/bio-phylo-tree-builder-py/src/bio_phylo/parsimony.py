"""
Maximum parsimony tree construction using Fitch's algorithm.

Provides:
- Fitch parsimony score calculation on a given tree topology
- Greedy stepwise addition heuristic for building parsimony trees
"""

from __future__ import annotations

from typing import Optional

from bio_phylo.tree import Node


# ======================================================================
# Fitch parsimony scoring
# ======================================================================


def fitch_score(tree: Node, alignment: dict[str, str]) -> int:
    """Compute the Fitch parsimony score for an alignment on a given tree.

    Parameters
    ----------
    tree : Node
        Rooted tree with leaf names matching keys in *alignment*.
    alignment : dict[str, str]
        {taxon_name: aligned_sequence}.

    Returns
    -------
    int
        Total number of character-state changes (the parsimony score).
    """
    tree_leaves = set(tree.leaf_names)
    align_leaves = set(alignment.keys())
    if tree_leaves != align_leaves:
        missing = tree_leaves - align_leaves
        extra = align_leaves - tree_leaves
        raise ValueError(f"Leaf/name mismatch: missing={missing}, extra={extra}")

    seq_len = len(next(iter(alignment.values())))
    total_score = 0
    for pos in range(seq_len):
        total_score += _fitch_downpass(tree, alignment, pos)
    return total_score


def _fitch_downpass(node: Node, alignment: dict[str, str], pos: int) -> int:
    """Fitch downpass for a single character. Returns the score increment."""
    if node.is_leaf:
        state = alignment[node.name][pos]
        node._fitch_state = set() if state in ("-", "N") else {state}  # type: ignore[attr-defined]
        return 0

    score = 0
    for child in node.children:
        score += _fitch_downpass(child, alignment, pos)

    child_states = [c._fitch_state for c in node.children]  # type: ignore[attr-defined]
    non_empty = [s for s in child_states if s]

    if not non_empty:
        node._fitch_state = set()  # type: ignore[attr-defined]
        return score

    intersection = non_empty[0]
    for s in non_empty[1:]:
        intersection = intersection & s

    if intersection:
        node._fitch_state = intersection  # type: ignore[attr-defined]
    else:
        union: set[str] = set()
        for s in non_empty:
            union |= s
        node._fitch_state = union  # type: ignore[attr-defined]
        score += 1

    return score


# ======================================================================
# Greedy stepwise addition heuristic
# ======================================================================


def parsimony_greedy(alignment: dict[str, str]) -> Node:
    """Build a parsimony tree using a greedy stepwise addition heuristic.

    Adds taxa one at a time, placing each in the position that minimally
    increases the parsimony score.
    """
    names = list(alignment.keys())
    if len(names) < 3:
        leaves = [Node(name=n, branch_length=0.0) for n in names]
        root = Node(children=leaves, branch_length=0.0)
        for leaf in leaves:
            leaf.parent = root
        return root

    # Start with the first 3 taxa as a trifurcation
    initial = names[:3]
    remaining = names[3:]

    root = Node(branch_length=0.0)
    leaves = [Node(name=n, branch_length=0.0) for n in initial]
    root.children = leaves
    for leaf in leaves:
        leaf.parent = root

    # Add remaining taxa one by one
    for taxon in remaining:
        root = _add_taxon_best(root, taxon, alignment)

    return root


def _add_taxon_best(
    tree: Node,
    taxon: str,
    alignment: dict[str, str],
) -> Node:
    """Try inserting a new taxon at every possible branch, return the best tree."""
    best_tree: Optional[Node] = None
    best_score = float("inf")

    # Get all current leaves
    leaves = [n for n in tree.all_nodes if n.is_leaf]

    for leaf in leaves:
        cand = tree.copy()
        cand_leaf = _find_leaf_by_name(cand, leaf.name)
        if cand_leaf is None or cand_leaf.parent is None:
            continue
        parent = cand_leaf.parent
        # Create new internal node between leaf and parent
        new_internal = Node(branch_length=0.0, children=[cand_leaf])
        new_internal.parent = parent
        cand_leaf.parent = new_internal
        parent.children = [new_internal if c is cand_leaf else c for c in parent.children]
        # Add new leaf as sister
        new_leaf = Node(name=taxon, branch_length=0.0)
        new_internal.children.append(new_leaf)
        new_leaf.parent = new_internal

        score = fitch_score(cand, alignment)
        if score < best_score:
            best_score = score
            best_tree = cand

    # If no valid placement found, add at root
    if best_tree is None:
        best_tree = tree.copy()
        new_leaf = Node(name=taxon, branch_length=0.0)
        best_tree.children.append(new_leaf)
        new_leaf.parent = best_tree

    return best_tree


def _find_leaf_by_name(root: Node, name: str) -> Optional[Node]:
    """Find a leaf node with the given name."""
    for node in root.postorder_iter():
        if node.is_leaf and node.name == name:
            return node
    return None
