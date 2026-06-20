"""Progressive Multiple Sequence Alignment.

Builds a guide tree from pairwise distances (UPGMA) and merges
alignments following the tree order.
"""

from __future__ import annotations

from itertools import combinations
from typing import Callable

from .align.result import AlignmentResult
from .align.nw import needleman_wunsch
from .matrices import get_matrix


# ── Distance matrix ──────────────────────────────────────────

def pairwise_distance_matrix(
    sequences: list[str],
    matrix: str = "simple",
    gap_penalty: int = -2,
    match: int = 2,
    mismatch: int = -1,
) -> list[list[float]]:
    """Compute a pairwise distance matrix using NW alignment.

    Distance = 1 - identity.
    """
    n = len(sequences)
    dist = [[0.0] * n for _ in range(n)]
    for i, j in combinations(range(n), 2):
        result = needleman_wunsch(
            sequences[i], sequences[j],
            matrix=matrix, gap_penalty=gap_penalty,
            match=match, mismatch=mismatch,
        )
        d = 1.0 - result.identity
        dist[i][j] = d
        dist[j][i] = d
    return dist


# ── UPGMA guide tree ────────────────────────────────────────

class TreeNode:
    """Node in a UPGMA guide tree."""

    def __init__(
        self,
        label: str | None = None,
        left: TreeNode | None = None,
        right: TreeNode | None = None,
        distance: float = 0.0,
    ) -> None:
        self.label = label
        self.left = left
        self.right = right
        self.distance = distance

    @property
    def is_leaf(self) -> bool:
        return self.left is None and self.right is None

    def leaves(self) -> list[str]:
        if self.is_leaf:
            return [self.label]  # type: ignore
        result: list[str] = []
        if self.left:
            result.extend(self.left.leaves())
        if self.right:
            result.extend(self.right.leaves())
        return result

    def __repr__(self) -> str:
        if self.is_leaf:
            return f"Leaf({self.label})"
        return f"Node({self.left!r}, {self.right!r}, d={self.distance:.3f})"


def upgma(dist: list[list[float]], labels: list[str]) -> TreeNode:
    """Build a UPGMA guide tree from a distance matrix.

    Parameters
    ----------
    dist : list[list[float]]
        Symmetric distance matrix with zero diagonal.
    labels : list[str]
        Labels for each sequence.

    Returns
    -------
    TreeNode
        Root of the guide tree.
    """
    n = len(labels)
    # Work with mutable copies
    clusters: list[TreeNode] = [TreeNode(label=l) for l in labels]
    sizes: list[int] = [1] * n
    D = [row[:] for row in dist]

    active = list(range(n))

    while len(active) > 1:
        # Find closest pair
        min_d = float("inf")
        ci, cj = active[0], active[1]
        for ia, a in enumerate(active):
            for b in active[ia + 1:]:
                if D[a][b] < min_d:
                    min_d = D[a][b]
                    ci, cj = a, b

        # Merge ci and cj
        new_node = TreeNode(
            left=clusters[ci],
            right=clusters[cj],
            distance=min_d / 2,
        )
        new_idx = len(clusters)
        clusters.append(new_node)
        sizes.append(sizes[ci] + sizes[cj])

        # Extend distance matrix
        new_row: list[float] = [0.0] * (len(D) + 1)
        for k in active:
            if k == ci or k == cj:
                continue
            # UPGMA: average linkage
            d_ik = D[ci][k] * sizes[ci] + D[cj][k] * sizes[cj]
            d_ik /= (sizes[ci] + sizes[cj])
            new_row[k] = d_ik
            if len(D[k]) <= new_idx:
                D[k].append(0.0)
            D[k][new_idx] = d_ik
        D.append(new_row)

        # Update active
        active.remove(ci)
        active.remove(cj)
        active.append(new_idx)

    return clusters[active[0]]


# ── Progressive alignment ───────────────────────────────────

def progressive_msa(
    sequences: list[str],
    labels: list[str] | None = None,
    matrix: str = "simple",
    gap_penalty: int = -2,
    match: int = 2,
    mismatch: int = -1,
) -> list[str]:
    """Perform progressive multiple sequence alignment.

    Parameters
    ----------
    sequences : list[str]
        Input sequences.
    labels : list[str], optional
        Labels for sequences. Defaults to Seq0, Seq1, ...
    matrix : str
        Substitution matrix name.
    gap_penalty : int

    Returns
    -------
    list[str]
        Aligned sequences (same order as input).
    """
    n = len(sequences)
    if labels is None:
        labels = [f"Seq{i}" for i in range(n)]

    if n == 0:
        return []
    if n == 1:
        return [sequences[0]]
    if n == 2:
        result = needleman_wunsch(
            sequences[0], sequences[1],
            matrix=matrix, gap_penalty=gap_penalty,
            match=match, mismatch=mismatch,
        )
        return [result.aligned_seq1, result.aligned_seq2]

    # Compute distance matrix and guide tree
    dist = pairwise_distance_matrix(
        sequences, matrix=matrix, gap_penalty=gap_penalty,
        match=match, mismatch=mismatch,
    )
    tree = upgma(dist, labels)

    # Align following the tree
    aligned = _align_tree(tree, sequences, labels, matrix, gap_penalty, match, mismatch)

    # Reorder to match input order
    label_to_aligned = dict(zip(labels, aligned))
    return [label_to_aligned[l] for l in labels]


def _align_tree(
    node: TreeNode,
    sequences: list[str],
    labels: list[str],
    matrix: str,
    gap_penalty: int,
    match: int,
    mismatch: int,
) -> list[str]:
    """Recursively align subtrees following the guide tree."""
    if node.is_leaf:
        idx = labels.index(node.label)  # type: ignore
        return [sequences[idx]]

    left_aligned = _align_tree(node.left, sequences, labels, matrix, gap_penalty, match, mismatch)  # type: ignore
    right_aligned = _align_tree(node.right, sequences, labels, matrix, gap_penalty, match, mismatch)  # type: ignore

    # Build consensus for each side to align
    left_consensus = _consensus(left_aligned)
    right_consensus = _consensus(right_aligned)

    # Align consensuses
    result = needleman_wunsch(
        left_consensus, right_consensus,
        matrix=matrix, gap_penalty=gap_penalty,
        match=match, mismatch=mismatch,
    )

    # Propagate gaps to all sequences on each side
    new_left = [_apply_gaps(seq, result.aligned_seq1, left_consensus) for seq in left_aligned]
    new_right = [_apply_gaps(seq, result.aligned_seq2, right_consensus) for seq in right_aligned]

    return new_left + new_right


def _consensus(seqs: list[str]) -> str:
    """Build a simple consensus from aligned sequences.

    For each column, pick the most common non-gap character, or '-'.
    """
    if not seqs:
        return ""
    length = len(seqs[0])
    consensus_chars: list[str] = []
    for col in range(length):
        chars = [s[col] for s in seqs if col < len(s)]
        non_gap = [c for c in chars if c != "-"]
        if non_gap:
            # most common
            from collections import Counter
            consensus_chars.append(Counter(non_gap).most_common(1)[0][0])
        else:
            consensus_chars.append("-")
    return "".join(consensus_chars)


def _apply_gaps(original: str, aligned_ref: str, ref_original: str) -> str:
    """Insert gaps into *original* at the same positions gaps were
    inserted into *ref_original* to produce *aligned_ref*.

    This is a positional mapping: we walk both original and aligned_ref,
    advancing through original only when a non-gap character appears.
    """
    result: list[str] = []
    orig_idx = 0

    for ch in aligned_ref:
        if ch == "-":
            # This is a gap inserted relative to the reference
            result.append("-")
        else:
            if orig_idx < len(original):
                result.append(original[orig_idx])
                orig_idx += 1
            else:
                result.append("-")

    # If original has remaining chars (shouldn't happen in correct alignment)
    while orig_idx < len(original):
        result.append(original[orig_idx])
        orig_idx += 1

    return "".join(result)
