"""
Tree data structure with Newick parsing and serialization.

Provides a Node-based phylogenetic tree with:
- Newick format parsing (with branch lengths and internal labels)
- Newick serialization
- Multiple traversals (preorder, postorder, level-order, leaf-only)
- Tree operations: rooting, rerooting, leaf/clade queries, topology stats
"""

from __future__ import annotations

import re
from collections import deque
from typing import Iterator, Optional


class Node:
    """A single node in a phylogenetic tree.

    Attributes:
        name: Taxon name (for leaves) or label (for internal nodes). Empty string if unnamed.
        branch_length: Distance from this node to its parent. None if unknown.
        children: Child nodes (empty list for leaves).
        parent: Reference to parent node (None for root).
    """

    def __init__(
        self,
        name: str = "",
        branch_length: Optional[float] = None,
        children: Optional[list[Node]] = None,
    ) -> None:
        self.name = name
        self.branch_length = branch_length
        self.children: list[Node] = children or []
        self.parent: Optional[Node] = None
        for child in self.children:
            child.parent = self

    # ------------------------------------------------------------------
    # Properties
    # ------------------------------------------------------------------

    @property
    def is_leaf(self) -> bool:
        return len(self.children) == 0

    @property
    def is_root(self) -> bool:
        return self.parent is None

    @property
    def num_leaves(self) -> int:
        if self.is_leaf:
            return 1
        return sum(c.num_leaves for c in self.children)

    @property
    def depth(self) -> int:
        """Maximum distance (in edges) from this node to any leaf."""
        if self.is_leaf:
            return 0
        return 1 + max(c.depth for c in self.children)

    @property
    def total_branch_length(self) -> float:
        """Sum of all branch lengths in the subtree rooted at this node."""
        bl = self.branch_length or 0.0
        return bl + sum(c.total_branch_length for c in self.children)

    @property
    def leaves(self) -> list[Node]:
        """Return all leaf descendants."""
        return list(self.leaf_iter())

    @property
    def leaf_names(self) -> list[str]:
        return [n.name for n in self.leaf_iter()]

    @property
    def all_nodes(self) -> list[Node]:
        return list(self.preorder_iter())

    # ------------------------------------------------------------------
    # Traversals
    # ------------------------------------------------------------------

    def preorder_iter(self) -> Iterator[Node]:
        """Root-first depth-first traversal."""
        yield self
        for child in self.children:
            yield from child.preorder_iter()

    def postorder_iter(self) -> Iterator[Node]:
        """Leaves-first depth-first traversal."""
        for child in self.children:
            yield from child.postorder_iter()
        yield self

    def levelorder_iter(self) -> Iterator[Node]:
        """Breadth-first traversal."""
        queue: deque[Node] = deque([self])
        while queue:
            node = queue.popleft()
            yield node
            for child in node.children:
                queue.append(child)

    def leaf_iter(self) -> Iterator[Node]:
        """Iterate over leaf nodes only (postorder)."""
        for node in self.postorder_iter():
            if node.is_leaf:
                yield node

    # ------------------------------------------------------------------
    # Clade helpers
    # ------------------------------------------------------------------

    def get_clade(self, leaf_names: set[str]) -> Node:
        """Return the smallest subtree containing exactly the given leaf names.

        Raises ValueError if the names don't map to a single clade.
        """
        my_leaves = set(self.leaf_names)
        if leaf_names == my_leaves:
            return self
        for child in self.children:
            child_leaves = set(child.leaf_names)
            if leaf_names <= child_leaves:
                return child.get_clade(leaf_names)
        raise ValueError(f"No single clade contains exactly {leaf_names}")

    def get_mrca(self, *nodes: Node) -> Node:
        """Most recent common ancestor of the given nodes.

        Uses the root-to-node path for each node and finds the last shared ancestor.
        """
        if not nodes:
            raise ValueError("Need at least one node")
        # Collect root-to-node paths
        paths: list[list[Node]] = []
        for n in nodes:
            path: list[Node] = []
            cur: Optional[Node] = n
            while cur is not None:
                path.append(cur)
                cur = cur.parent
            path.reverse()
            paths.append(path)
        # Walk down until divergence
        ancestor = paths[0][0]
        for depth in range(1, min(len(p) for p in paths)):
            if all(paths[i][depth] is paths[0][depth] for i in range(len(paths))):
                ancestor = paths[0][depth]
            else:
                break
        return ancestor

    # ------------------------------------------------------------------
    # Topology
    # ------------------------------------------------------------------

    def num_internal_nodes(self) -> int:
        return sum(1 for n in self.preorder_iter() if not n.is_leaf)

    def is_binary(self) -> bool:
        """True if every internal node has exactly 2 children (strict binary)."""
        for node in self.preorder_iter():
            if not node.is_leaf and len(node.children) != 2:
                return False
        return True

    def height(self) -> float:
        """Longest root-to-leaf distance (sum of branch lengths)."""
        if self.is_leaf:
            return self.branch_length or 0.0
        child_heights = [c.height() for c in self.children]
        max_h = max(child_heights)
        return (self.branch_length or 0.0) + max_h

    # ------------------------------------------------------------------
    # Rooting / rerooting
    # ------------------------------------------------------------------

    def root_at(self, node: Node) -> Node:
        """Reroot the tree so that *node* becomes the new root.

        Branch lengths are split on the edge leading to *node* to preserve
        additive distances.

        Returns the new root node.
        """
        if node is self:
            return self  # already root

        # Collect the path from the old root to the new root
        path: list[Node] = []
        cur: Node = node
        while cur is not None:
            path.append(cur)
            cur = cur.parent  # type: ignore[assignment]
        path.reverse()  # root → … → new_root

        # Walk down path, reversing parent/child and splitting branch lengths
        for i in range(len(path) - 1):
            parent = path[i]
            child = path[i + 1]
            # Split branch length of child between the two sides
            bl = child.branch_length or 0.0
            half = bl / 2.0
            child.branch_length = half
            # Reverse relationship
            parent.children.remove(child)
            child.children.append(parent)
            parent.parent = child
        node.parent = None  # new root
        return node

    @staticmethod
    def root_at_midpoint(tree: Node) -> Node:
        """Create a new tree rooted at the midpoint of the longest path.

        Returns a fresh root node; the original tree is not modified.
        """
        leaves = tree.leaves
        # Find two most distant leaves by summing branch lengths along the path
        max_dist = -1.0
        far_a: Node = leaves[0]
        far_b: Node = leaves[0]
        for i, a in enumerate(leaves):
            for b in leaves[i + 1 :]:
                d = _path_length(a, b)
                if d > max_dist:
                    max_dist = d
                    far_a, far_b = a, b
        # Walk from far_a toward far_b for half the distance
        target = max_dist / 2.0
        cur = far_a
        acc = 0.0
        while True:
            parent = cur.parent
            if parent is None:
                break
            bl = cur.branch_length or 0.0
            if acc + bl >= target - 1e-9:
                # Split the branch
                remain = target - acc
                # Create a new internal node on this branch
                new_root = Node(branch_length=0.0)
                cur.branch_length = bl - remain
                new_root.children.append(cur)
                cur.parent = new_root
                # Attach the rest of the old tree as the other child
                parent.children.remove(cur)
                new_root.children.append(parent)
                parent.parent = new_root
                new_root.parent = None
                return new_root
            acc += bl
            cur = parent
        # Fallback: just root at the midpoint node found
        tree.root_at(cur)
        return tree

    # ------------------------------------------------------------------
    # Newick serialization
    # ------------------------------------------------------------------

    def to_newick(self, precision: int = 6, include_root_bl: bool = True) -> str:
        """Serialize to Newick format string (with trailing semicolon)."""
        return self._to_newick_inner(precision) + ";"

    def _to_newick_inner(self, precision: int) -> str:
        """Internal serialization without semicolon."""
        parts: list[str] = []
        if self.is_leaf:
            parts.append(_escape_name(self.name))
        else:
            child_strs = [c._to_newick_inner(precision=precision) for c in self.children]
            parts.append("(" + ",".join(child_strs) + ")")
            if self.name:
                parts.append(_escape_name(self.name))
        if self.branch_length is not None:
            parts.append(f":{self.branch_length:.{precision}f}")
        return "".join(parts)

    @staticmethod
    def from_newick(newick: str) -> Node:
        """Parse a Newick string into a Node tree.

        Handles branch lengths, internal node labels, leaf names, and trailing semicolons.
        """
        newick = newick.strip()
        if not newick:
            raise ValueError("Empty Newick string")
        if newick.endswith(";"):
            newick = newick[:-1]
        parser = _NewickParser(newick)
        return parser.parse()

    # ------------------------------------------------------------------
    # Deep copy
    # ------------------------------------------------------------------

    def copy(self) -> Node:
        """Return a deep copy of the subtree."""
        children_copy = [c.copy() for c in self.children]
        node = Node(name=self.name, branch_length=self.branch_length, children=children_copy)
        return node

    # ------------------------------------------------------------------
    # String representation
    # ------------------------------------------------------------------

    def __repr__(self) -> str:
        if self.is_leaf:
            return f"Node({self.name!r}, bl={self.branch_length})"
        return (
            f"Node(name={self.name!r}, children={len(self.children)}, "
            f"bl={self.branch_length})"
        )

    def __str__(self) -> str:
        return self.to_newick(precision=4)


# ======================================================================
# Module-level helpers
# ======================================================================


def _escape_name(name: str) -> str:
    """Wrap a name in single quotes if it contains special characters."""
    if not name:
        return ""
    safe = re.compile(r"^[A-Za-z0-9_.-]+$")
    if safe.match(name):
        return name
    return "'" + name.replace("'", "''") + "'"


def _path_length(a: Node, b: Node) -> float:
    """Sum of branch lengths along the path between two nodes."""
    # Find MRCA
    ancestors_a: set[int] = set()
    cur: Optional[Node] = a
    while cur is not None:
        ancestors_a.add(id(cur))
        cur = cur.parent
    # Walk from b up until we hit the MRCA
    cur = b
    dist = 0.0
    while cur is not None:
        if id(cur) in ancestors_a:
            # Walk from a up to MRCA
            cur_a: Optional[Node] = a
            while cur_a is not None:
                if cur_a is cur:
                    break
                dist += cur_a.branch_length or 0.0
                cur_a = cur_a.parent
            break
        dist += cur.branch_length or 0.0
        cur = cur.parent
    return dist


class _NewickParser:
    """Recursive-descent parser for Newick format."""

    def __init__(self, s: str) -> None:
        self.s = s
        self.pos = 0

    def peek(self) -> str:
        self._skip_spaces()
        if self.pos < len(self.s):
            return self.s[self.pos]
        return ""

    def consume(self, expected: str) -> None:
        self._skip_spaces()
        if self.pos >= len(self.s) or self.s[self.pos] != expected:
            pos = self.pos
            raise ValueError(
                f"Expected '{expected}' at position {pos}, got "
                f"{self.s[pos:pos + 20]!r}"
            )
        self.pos += 1

    def _skip_spaces(self) -> None:
        while self.pos < len(self.s) and self.s[self.pos] == " ":
            self.pos += 1

    def parse(self) -> Node:
        node = self._parse_subtree()
        # Consume trailing semicolon if present
        self._skip_spaces()
        if self.pos < len(self.s) and self.s[self.pos] == ";":
            self.pos += 1
        return node

    def _parse_subtree(self) -> Node:
        ch = self.peek()
        if ch == "(":
            return self._parse_internal()
        else:
            return self._parse_leaf()

    def _parse_leaf(self) -> Node:
        name = self._parse_name()
        bl = self._maybe_branch_length()
        return Node(name=name, branch_length=bl)

    def _parse_internal(self) -> Node:
        self.consume("(")
        children: list[Node] = [self._parse_subtree()]
        while self.peek() == ",":
            self.consume(",")
            children.append(self._parse_subtree())
        self.consume(")")
        name = self._parse_name()
        bl = self._maybe_branch_length()
        return Node(name=name, branch_length=bl, children=children)

    def _parse_name(self) -> str:
        self._skip_spaces()
        if self.pos >= len(self.s):
            return ""
        ch = self.s[self.pos]
        if ch in ("(", ")", ",", ":", ";"):
            return ""
        if ch == "'":
            return self._parse_quoted_name()
        # Unquoted name: read until a delimiter
        start = self.pos
        while self.pos < len(self.s) and self.s[self.pos] not in ("(", ")", ",", ":", ";", " "):
            self.pos += 1
        return self.s[start : self.pos]

    def _parse_quoted_name(self) -> str:
        self.consume("'")
        parts: list[str] = []
        while self.pos < len(self.s):
            ch = self.s[self.pos]
            if ch == "'":
                if self.pos + 1 < len(self.s) and self.s[self.pos + 1] == "'":
                    parts.append("'")
                    self.pos += 2
                else:
                    self.pos += 1  # closing quote
                    break
            else:
                parts.append(ch)
                self.pos += 1
        return "".join(parts)

    def _maybe_branch_length(self) -> Optional[float]:
        self._skip_spaces()
        if self.pos < len(self.s) and self.s[self.pos] == ":":
            self.pos += 1
            start = self.pos
            while self.pos < len(self.s) and self.s[self.pos] not in (",", ")", ";", " "):
                self.pos += 1
            return float(self.s[start : self.pos])
        return None


# ======================================================================
# Convenience constructors
# ======================================================================


def from_newick(newick: str) -> Node:
    """Parse a Newick string and return the root Node."""
    return Node.from_newick(newick)


def from_leaf_names(names: list[str]) -> Node:
    """Create an unrooted star tree (polytomy) from a list of leaf names.

    All branch lengths are zero.
    """
    leaves = [Node(name=n, branch_length=0.0) for n in names]
    return Node(children=leaves, branch_length=0.0)
