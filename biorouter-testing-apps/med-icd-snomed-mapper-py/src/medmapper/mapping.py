"""
mapping.py – Crosswalk engine for ICD-10 ↔ SNOMED CT (and other terminologies).

Features:
  - One-to-one mapping
  - One-to-many mapping with group / rule / priority
  - Bidirectional lookup (build reverse index automatically)
  - Mapping result objects carrying provenance metadata
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple

from .terminology import MapEntry, TerminologyStore, Concept


# ── result objects ───────────────────────────────────────────────────────────

@dataclass(frozen=True)
class MappingResult:
    """One mapped target code, with provenance."""

    target_terminology: str
    target_code: str
    target_description: str = ""
    map_group: int = 1
    map_rule: str = ""
    map_priority: int = 1
    map_category: str = ""


@dataclass
class CrosswalkResult:
    """Aggregated result of a crosswalk query."""

    source_terminology: str
    source_code: str
    source_description: str = ""
    mappings: List[MappingResult] = field(default_factory=list)

    @property
    def best(self) -> Optional[MappingResult]:
        """Return the highest-priority (lowest number) mapping, or None."""
        if not self.mappings:
            return None
        return sorted(self.mappings, key=lambda m: m.map_priority)[0]

    @property
    def is_one_to_one(self) -> bool:
        return len(self.mappings) == 1


# ── crosswalk engine ─────────────────────────────────────────────────────────

class CrosswalkEngine:
    """
    Bidirectional crosswalk between terminologies using a mapping table.

    Parameters
    ----------
    store : TerminologyStore
        The concept store (used to resolve descriptions).
    entries : list[MapEntry]
        The mapping rows.  Both directions may be present; the engine
        automatically builds reverse indices.
    """

    def __init__(self, store: TerminologyStore, entries: List[MapEntry]) -> None:
        self._store = store
        self._entries = list(entries)
        # forward: source_key -> [MapEntry, ...]
        self._forward: Dict[Tuple[str, str], List[MapEntry]] = {}
        # reverse: target_key -> [MapEntry, ...]
        self._reverse: Dict[Tuple[str, str], List[MapEntry]] = {}

        self._build_indices()

    def _build_indices(self) -> None:
        for entry in self._entries:
            self._forward.setdefault(entry.source_key, []).append(entry)
            self._reverse.setdefault(entry.target_key, []).append(entry)

    def _resolve_description(self, terminology: str, code: str) -> str:
        concept = self._store.get(terminology, code)
        return concept.description if concept else ""

    def _entries_to_results(self, entries: List[MapEntry]) -> List[MappingResult]:
        results: List[MappingResult] = []
        for e in sorted(entries, key=lambda x: (x.map_group, x.map_priority)):
            results.append(
                MappingResult(
                    target_terminology=e.target_terminology,
                    target_code=e.target_code,
                    target_description=self._resolve_description(e.target_terminology, e.target_code),
                    map_group=e.map_group,
                    map_rule=e.map_rule,
                    map_priority=e.map_priority,
                    map_category=e.map_category,
                )
            )
        return results

    # ── public API ───────────────────────────────────────────────────────

    def map_code(
        self,
        source_terminology: str,
        source_code: str,
        target_terminology: Optional[str] = None,
    ) -> CrosswalkResult:
        """
        Map a single source code to target terminology codes.

        If ``target_terminology`` is given, only mappings to that target
        are returned.  Otherwise all available mappings are returned.
        """
        source_key = (source_terminology, source_code)
        source_desc = self._resolve_description(source_terminology, source_code)

        raw = self._forward.get(source_key, [])
        if target_terminology:
            raw = [e for e in raw if e.target_terminology == target_terminology]

        return CrosswalkResult(
            source_terminology=source_terminology,
            source_code=source_code,
            source_description=source_desc,
            mappings=self._entries_to_results(raw),
        )

    def reverse_lookup(
        self,
        target_terminology: str,
        target_code: str,
        source_terminology: Optional[str] = None,
    ) -> CrosswalkResult:
        """
        Reverse lookup: given a target code, find source codes that map to it.
        """
        target_key = (target_terminology, target_code)
        target_desc = self._resolve_description(target_terminology, target_code)

        raw = self._reverse.get(target_key, [])
        if source_terminology:
            raw = [e for e in raw if e.source_terminology == source_terminology]

        # For reverse, swap source/target in the result
        results: List[MappingResult] = []
        for e in sorted(raw, key=lambda x: (x.map_group, x.map_priority)):
            results.append(
                MappingResult(
                    target_terminology=e.source_terminology,
                    target_code=e.source_code,
                    target_description=self._resolve_description(e.source_terminology, e.source_code),
                    map_group=e.map_group,
                    map_rule=e.map_rule,
                    map_priority=e.map_priority,
                    map_category=e.map_category,
                )
            )

        return CrosswalkResult(
            source_terminology=target_terminology,
            source_code=target_code,
            source_description=target_desc,
            mappings=results,
        )

    def has_mapping(
        self, source_terminology: str, source_code: str, target_terminology: Optional[str] = None
    ) -> bool:
        """Check if a mapping exists for the given source code."""
        result = self.map_code(source_terminology, source_code, target_terminology)
        return len(result.mappings) > 0

    @property
    def entry_count(self) -> int:
        return len(self._entries)

    def __repr__(self) -> str:
        return f"CrosswalkEngine(entries={self.entry_count})"
