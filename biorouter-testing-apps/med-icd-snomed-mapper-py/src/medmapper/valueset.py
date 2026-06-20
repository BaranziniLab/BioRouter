"""
valueset.py – Value-set expansion: given a root concept, expand to all descendants.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import List, Optional, Set, Tuple

from .hierarchy import Hierarchy
from .terminology import Concept, TerminologyStore


@dataclass
class ValueSet:
    """An expanded value set rooted at a specific concept."""

    root_terminology: str
    root_code: str
    root_description: str = ""
    members: List[Concept] = field(default_factory=list)
    include_root: bool = True

    @property
    def size(self) -> int:
        return len(self.members)

    @property
    def codes(self) -> List[str]:
        return [m.code for m in self.members]

    def contains(self, terminology: str, code: str) -> bool:
        return any(m.terminology == terminology and m.code == code for m in self.members)

    def __repr__(self) -> str:
        return (
            f"ValueSet(root={self.root_terminology}:{self.root_code}, "
            f"members={self.size})"
        )


class ValueSetExpander:
    """
    Expands a root concept to a value set containing all descendants
    (and optionally the root itself) via the Hierarchy.

    Parameters
    ----------
    store : TerminologyStore
        Concept registry.
    hierarchy : Hierarchy
        The hierarchy graph.
    """

    def __init__(self, store: TerminologyStore, hierarchy: Hierarchy) -> None:
        self._store = store
        self._hierarchy = hierarchy

    def expand(
        self,
        terminology: str,
        root_code: str,
        include_root: bool = True,
        max_depth: Optional[int] = None,
    ) -> ValueSet:
        """
        Expand *root_code* to all descendants.

        Parameters
        ----------
        terminology : str
            The terminology namespace.
        root_code : str
            The root concept code.
        include_root : bool
            Whether to include the root itself in the member list.
        max_depth : int, optional
            If set, limit expansion to this many levels below the root.
        """
        root = self._store.get(terminology, root_code)
        if root is None:
            return ValueSet(
                root_terminology=terminology,
                root_code=root_code,
                root_description="",
                members=[],
                include_root=include_root,
            )

        raw = self._hierarchy.descendants(terminology, root_code, include_self=include_root)

        members: List[Concept] = []
        for tkey, code in raw:
            concept = self._store.get(tkey, code)
            if concept is None:
                continue

            if max_depth is not None:
                depth = self._hierarchy.depth(tkey, code)
                root_depth = self._hierarchy.depth(terminology, root_code)
                if depth - root_depth > max_depth:
                    continue

            members.append(concept)

        return ValueSet(
            root_terminology=terminology,
            root_code=root_code,
            root_description=root.description,
            members=members,
            include_root=include_root,
        )

    def expand_multiple(
        self,
        terminology: str,
        root_codes: List[str],
        include_root: bool = True,
    ) -> ValueSet:
        """
        Expand several root codes and merge into one value set.
        De-duplicates by code.
        """
        seen: Set[str] = set()
        all_members: List[Concept] = []
        root_descs: List[str] = []

        for rc in root_codes:
            vs = self.expand(terminology, rc, include_root=include_root)
            root_descs.append(vs.root_description)
            for m in vs.members:
                if m.code not in seen:
                    seen.add(m.code)
                    all_members.append(m)

        return ValueSet(
            root_terminology=terminology,
            root_code=",".join(root_codes),
            root_description=" + ".join(root_descs),
            members=all_members,
            include_root=include_root,
        )
