"""
UPGMA (Unweighted Pair Group Method with Arithmetic Mean).

Implements the UPGMA algorithm for constructing ultrametric trees
(constant molecular clock assumption) from a pairwise distance matrix.
"""

from __future__ import annotations

from bio_phylo.distance import DistanceMatrix
from bio_phylo.tree import Node


def upgma(dm: DistanceMatrix) -> Node:
    """Build an ultrametric tree using the UPGMA algorithm.

    Parameters
    ----------
    dm : DistanceMatrix
        Symmetric pairwise distance matrix.

    Returns
    -------
    Node
        Root of the UPGMA tree. All root-to-leaf paths have equal total
        branch length (ultrametric property).

    Algorithm
    ---------
    1. Start with each taxon as a singleton cluster.
    2. Find the two closest clusters.
    3. Join them under a new internal node placed at half the distance.
    4. Recompute distances from the new cluster to all others using
       the arithmetic mean (UPGMA weighting).
    5. Repeat until one cluster remains.
    """
    names = list(dm.names)
    n = len(names)

    # Working copy of the distance matrix (list-of-dicts for mutability)
    dists: dict[str, dict[str, float]] = {name: dict(dm._matrix[name]) for name in names}

    # Map from cluster name → Node (leaf or internal)
    nodes: dict[str, Node] = {name: Node(name=name, branch_length=0.0) for name in names}

    # Map from cluster name → number of original taxa (for mean weighting)
    sizes: dict[str, int] = {name: 1 for name in names}

    active = list(names)

    while len(active) > 1:
        # Find the minimum distance pair
        min_dist = float("inf")
        min_i, min_j = -1, -1
        for i in range(len(active)):
            for j in range(i + 1, len(active)):
                d = dists[active[i]][active[j]]
                if d < min_dist:
                    min_dist = d
                    min_i, min_j = i, j

        a_name = active[min_i]
        b_name = active[min_j]
        new_name = f"({a_name},{b_name})"
        new_size = sizes[a_name] + sizes[b_name]

        # Branch lengths: half the distance between the two clusters
        bl_a = min_dist / 2.0 - _cluster_height(a_name, nodes, dists)
        bl_b = min_dist / 2.0 - _cluster_height(b_name, nodes, dists)
        if bl_a < 0:
            bl_a = 0.0
        if bl_b < 0:
            bl_b = 0.0

        nodes[a_name].branch_length = bl_a
        nodes[b_name].branch_length = bl_b

        # Create new internal node
        new_node = Node(
            name=new_name,
            branch_length=0.0,
            children=[nodes[a_name], nodes[b_name]],
        )
        nodes[a_name].parent = new_node
        nodes[b_name].parent = new_node
        nodes[new_name] = new_node
        sizes[new_name] = new_size

        # Compute distances from the new cluster to all other active clusters
        dists[new_name] = {}
        for k in active:
            if k == a_name or k == b_name:
                continue
            # UPGMA: arithmetic mean weighted by cluster sizes
            d_ak = dists[a_name][k]
            d_bk = dists[b_name][k]
            d_new = (sizes[a_name] * d_ak + sizes[b_name] * d_bk) / new_size
            dists[new_name][k] = d_new
            dists[k][new_name] = d_new
        dists[new_name][new_name] = 0.0

        # Remove old clusters from active set, add new one
        active.pop(max(min_i, min_j))
        active.pop(min(min_i, min_j))
        active.append(new_name)

    root = nodes[active[0]]
    return root


def _cluster_height(
    name: str, nodes: dict[str, Node], dists: dict[str, dict[str, float]]
) -> float:
    """Compute the height (distance from leaves) of a cluster node."""
    node = nodes[name]
    if node.is_leaf:
        return 0.0
    leaves = node.leaf_names
    if len(leaves) < 2:
        return 0.0
    total = 0.0
    count = 0
    for i in range(len(leaves)):
        for j in range(i + 1, len(leaves)):
            d = dists.get(leaves[i], {}).get(leaves[j], 0.0)
            total += d
            count += 1
    return total / (2.0 * count) if count > 0 else 0.0
