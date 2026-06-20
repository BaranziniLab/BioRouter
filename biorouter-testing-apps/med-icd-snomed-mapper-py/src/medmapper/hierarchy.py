"""
hierarchy.py – Directed-acyclic-graph operations over clinical code hierarchies.

Provides:
  - Hierarchy: built from parent_codes on each Concept
  - Operations: ancestors, descendants, is_a, lowest_common_ancestor, depth
"""

from __future__ import annotations

from collections import deque
from typing import Dict, List, Optional, Set, Tuple

from .terminology import Concept, TerminologyStore


class Hierarchy:
    """
    Maintains parent → child adjacency lists derived from Concept.parent_codes.

    Works across multiple terminologies; each node is identified by
    (terminology, code) tuple.
    """

    def __init__(self, store: TerminologyStore) -> None:
        self._store = store
        # child -> set of parents  (parent_codes on the Concept)
        self._parents: Dict[Tuple[str, str], Set[Tuple[str, str]]] = {}
        # parent -> set of children
        self._children: Dict[Tuple[str, str], Set[Tuple[str, str]]] = {}

        self._build()

    # ── construction ─────────────────────────────────────────────────────

    def _build(self) -> None:
        for concept in self._store.all_concepts():
            node = concept.key
            self._parents.setdefault(node, set())
            self._children.setdefault(node, set())
            for pc in concept.parent_codes:
                parent_key = (concept.terminology, pc)
                self._parents.setdefault(node, set()).add(parent_key)
                self._children.setdefault(parent_key, set()).add(node)

    # ── queries ──────────────────────────────────────────────────────────

    def parents(self, terminology: str, code: str) -> Set[Tuple[str, str]]:
        """Immediate parents of a code."""
        return set(self._parents.get((terminology, code), set()))

    def children(self, terminology: str, code: str) -> Set[Tuple[str, str]]:
        """Immediate children of a code."""
        return set(self._children.get((terminology, code), set()))

    def ancestors(self, terminology: str, code: str, include_self: bool = False) -> List[Tuple[str, str]]:
        """All ancestors (BFS upward).  Order: breadth-first from root."""
        root = (terminology, code)
        visited: Set[Tuple[str, str]] = set() if not include_self else {root}
        queue = deque(self._parents.get(root, set()))
        result: List[Tuple[str, str]] = []
        while queue:
            node = queue.popleft()
            if node in visited:
                continue
            visited.add(node)
            result.append(node)
            queue.extend(self._parents.get(node, set()))
        return result

    def descendants(self, terminology: str, code: str, include_self: bool = False) -> List[Tuple[str, str]]:
        """All descendants (BFS downward)."""
        root = (terminology, code)
        visited: Set[Tuple[str, str]] = set() if not include_self else {root}
        queue = deque(self._children.get(root, set()))
        result: List[Tuple[str, str]] = []
        while queue:
            node = queue.popleft()
            if node in visited:
                continue
            visited.add(node)
            result.append(node)
            queue.extend(self._children.get(node, set()))
        return result

    def is_a(self, child: Tuple[str, str], ancestor: Tuple[str, str]) -> bool:
        """True if *child* is (transitively) a descendant of *ancestor*."""
        if child == ancestor:
            return True
        visited: Set[Tuple[str, str]] = set()
        queue = deque(self._parents.get(child, set()))
        while queue:
            node = queue.popleft()
            if node == ancestor:
                return True
            if node in visited:
                continue
            visited.add(node)
            queue.extend(self._parents.get(node, set()))
        return False

    def lowest_common_ancestor(
        self, terminology: str, code_a: str, code_b: str
    ) -> Optional[Tuple[str, str]]:
        """
        Compute the lowest common ancestor of two codes within the same terminology.

        Returns None if the codes are in disconnected sub-trees.
        """
        a = (terminology, code_a)
        b = (terminology, code_b)

        if a == b:
            return a

        # BFS from both nodes upward, meeting at the first shared ancestor.
        visited_a: Dict[Tuple[str, str], int] = {a: 0}
        visited_b: Dict[Tuple[str, str], int] = {b: 0}
        queue_a = deque([(a, 0)])
        queue_b = deque([(b, 0)])

        while queue_a or queue_b:
            # expand the shallower frontier
            if queue_a:
                node, depth_a = queue_a.popleft()
                for parent in self._parents.get(node, set()):
                    if parent in visited_b:
                        return parent
                    if parent not in visited_a:
                        visited_a[parent] = depth_a + 1
                        queue_a.append((parent, depth_a + 1))

            if queue_b:
                node, depth_b = queue_b.popleft()
                for parent in self._parents.get(node, set()):
                    if parent in visited_a:
                        return parent
                    if parent not in visited_b:
                        visited_b[parent] = depth_b + 1
                        queue_b.append((parent, depth_b + 1))

        return None

    def depth(self, terminology: str, code: str) -> int:
        """Distance from the deepest root ancestor."""
        ancestors = self.ancestors(terminology, code)
        if not ancestors:
            return 0
        return len(ancestors)

    def roots(self, terminology: str) -> List[Tuple[str, str]]:
        """Return codes that have no parents (roots of the hierarchy)."""
        return [
            (terminology, code)
            for code in self._store.codes_for(terminology)
            if not self._parents.get((terminology, code), set())
        ]

    def __repr__(self) -> str:
        return f"Hierarchy(nodes={len(self._parents)})"
