"""
Neighbor-Joining (NJ) tree construction.

Implements the Saitou & Nei (1987) algorithm for building additive
(non-ultrametric) trees from a pairwise distance matrix.
"""

from __future__ import annotations

from bio_phylo.distance import DistanceMatrix
from bio_phylo.tree import Node


def neighbor_joining(dm: DistanceMatrix) -> Node:
    """Build a tree using the Neighbor-Joining algorithm.

    Parameters
    ----------
    dm : DistanceMatrix
        Symmetric pairwise distance matrix.

    Returns
    -------
    Node
        Root of the NJ tree. Unlike UPGMA, this tree is NOT ultrametric:
        branch lengths reflect estimated evolutionary distances.

    Algorithm
    ---------
    1. Compute the net divergence r(i) for each taxon.
    2. Compute the corrected distance matrix Q.
    3. Find the pair (i, j) with the smallest Q value.
    4. Create a new node connecting i and j with computed branch lengths.
    5. Update the distance matrix with distances from the new node.
    6. Repeat until 3 nodes remain, then join them in a trifurcation.
    """
    names = list(dm.names)
    n = len(names)

    # Working copy
    dists: dict[str, dict[str, float]] = {name: dict(dm._matrix[name]) for name in names}
    active = list(names)
    node_map: dict[str, Node] = {name: Node(name=name, branch_length=0.0) for name in names}

    while len(active) > 3:
        k = len(active)
        # Step 1: Compute net divergences
        r = {}
        for taxon in active:
            r[taxon] = sum(dists[taxon][other] for other in active if other != taxon)

        # Step 2: Compute Q matrix
        q_min = float("inf")
        q_pair = (active[0], active[1])
        for i in range(k):
            for j in range(i + 1, k):
                a, b = active[i], active[j]
                q = (k - 2) * dists[a][b] - r[a] - r[b]
                if q < q_min:
                    q_min = q
                    q_pair = (a, b)

        # Step 3: Find the neighbor pair
        i_name, j_name = q_pair

        # Step 4: Compute branch lengths
        bl_i = dists[i_name][j_name] / 2.0 + (r[i_name] - r[j_name]) / (2.0 * (k - 2))
        bl_j = dists[i_name][j_name] - bl_i
        if bl_i < 0:
            bl_i = 0.0
        if bl_j < 0:
            bl_j = 0.0

        # Create new node
        new_name = f"({i_name},{j_name})"
        new_node = Node(
            name=new_name,
            branch_length=0.0,
            children=[node_map[i_name], node_map[j_name]],
        )
        node_map[i_name].branch_length = bl_i
        node_map[j_name].branch_length = bl_j
        node_map[i_name].parent = new_node
        node_map[j_name].parent = new_node
        node_map[new_name] = new_node

        # Step 5: Compute distances from new node to all others
        dists[new_name] = {}
        dists[new_name][new_name] = 0.0
        for m in active:
            if m == i_name or m == j_name:
                continue
            d = (dists[i_name][m] + dists[j_name][m] - dists[i_name][j_name]) / 2.0
            dists[new_name][m] = d
            dists[m][new_name] = d

        # Update active list
        active.remove(i_name)
        active.remove(j_name)
        active.append(new_name)

    # Step 6: Last 3 nodes — join in a trifurcation
    a, b, c = active[0], active[1], active[2]
    # Create the root
    root_name = f"({a},{b},{c})"
    root = Node(name=root_name, branch_length=0.0)

    # Branch lengths for the final trifurcation
    bl_a = (dists[a][b] + dists[a][c] - dists[b][c]) / 2.0
    bl_b = (dists[a][b] + dists[b][c] - dists[a][c]) / 2.0
    bl_c = (dists[a][c] + dists[b][c] - dists[a][b]) / 2.0

    node_map[a].branch_length = max(bl_a, 0.0)
    node_map[b].branch_length = max(bl_b, 0.0)
    node_map[c].branch_length = max(bl_c, 0.0)

    node_map[a].parent = root
    node_map[b].parent = root
    node_map[c].parent = root
    root.children = [node_map[a], node_map[b], node_map[c]]

    return root
