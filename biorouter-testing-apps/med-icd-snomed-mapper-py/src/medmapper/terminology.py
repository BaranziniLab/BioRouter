"""
terminology.py – Core data models and in-memory store for clinical codes.

Provides:
  - Concept: immutable representation of a single code (code, description, terminology, active flag)
  - TerminologyStore: in-memory registry keyed by (terminology, code) with convenience lookups
  - CSV / JSON loaders for bootstrap data
"""

from __future__ import annotations

import csv
import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Sequence, Tuple

# ── data models ──────────────────────────────────────────────────────────────

@dataclass(frozen=True, order=True)
class Concept:
    """A single clinical concept/code."""

    code: str
    description: str
    terminology: str  # "ICD-10-CM" | "SNOMED-CT" | …
    active: bool = True
    parent_codes: tuple[str, ...] = ()  # immediate parents in the hierarchy

    @property
    def key(self) -> Tuple[str, str]:
        return (self.terminology, self.code)


@dataclass
class MapEntry:
    """One row in a cross-terminology mapping table."""

    source_terminology: str
    source_code: str
    target_terminology: str
    target_code: str
    map_group: int = 1
    map_rule: str = ""        # e.g. "AND" / "OR" / "" (unconditional)
    map_priority: int = 1      # lower = preferred
    map_category: str = ""     # "equivalent", "narrower", "broader", etc.

    @property
    def source_key(self) -> Tuple[str, str]:
        return (self.source_terminology, self.source_code)

    @property
    def target_key(self) -> Tuple[str, str]:
        return (self.target_terminology, self.target_code)


# ── terminology store ────────────────────────────────────────────────────────

class TerminologyStore:
    """In-memory registry of concepts, indexed by (terminology, code)."""

    def __init__(self) -> None:
        self._concepts: Dict[Tuple[str, str], Concept] = {}
        self._by_terminology: Dict[str, Dict[str, Concept]] = {}

    # ── mutation ─────────────────────────────────────────────────────────

    def add(self, concept: Concept) -> None:
        key = concept.key
        self._concepts[key] = concept
        self._by_terminology.setdefault(concept.terminology, {})[concept.code] = concept

    def add_many(self, concepts: Sequence[Concept]) -> None:
        for c in concepts:
            self.add(c)

    # ── lookups ──────────────────────────────────────────────────────────

    def get(self, terminology: str, code: str) -> Optional[Concept]:
        return self._concepts.get((terminology, code))

    def is_valid(self, terminology: str, code: str) -> bool:
        c = self.get(terminology, code)
        return c is not None and c.active

    def codes_for(self, terminology: str) -> List[str]:
        return list(self._by_terminology.get(terminology, {}).keys())

    def concepts_for(self, terminology: str) -> List[Concept]:
        return list(self._by_terminology.get(terminology, {}).values())

    def all_concepts(self) -> List[Concept]:
        return list(self._concepts.values())

    def __len__(self) -> int:
        return len(self._concepts)

    def __contains__(self, key: Tuple[str, str]) -> bool:
        return key in self._concepts

    def __repr__(self) -> str:
        counts = {t: len(d) for t, d in self._by_terminology.items()}
        return f"TerminologyStore({counts})"


# ── loaders ──────────────────────────────────────────────────────────────────

def load_concepts_csv(path: str | Path, terminology: str) -> List[Concept]:
    """
    Load concepts from a CSV with columns: code, description, parent_codes (optional, semicolon-delimited).
    ``terminology`` is applied to every row.
    """
    path = Path(path)
    concepts: List[Concept] = []
    with path.open(newline="", encoding="utf-8") as fh:
        reader = csv.DictReader(fh)
        for row in reader:
            raw_parents = row.get("parent_codes", "").strip()
            parents = tuple(p.strip() for p in raw_parents.split(";") if p.strip())
            active_val = row.get("active", "true").strip().lower()
            concepts.append(
                Concept(
                    code=row["code"].strip(),
                    description=row["description"].strip(),
                    terminology=terminology,
                    active=active_val in ("true", "1", "yes"),
                    parent_codes=parents,
                )
            )
    return concepts


def load_concepts_json(path: str | Path) -> List[Concept]:
    """
    Load concepts from a JSON list of objects.
    Each object must have: code, description, terminology.
    Optional: active (bool), parent_codes (list[str]).
    """
    path = Path(path)
    with path.open(encoding="utf-8") as fh:
        data = json.load(fh)
    concepts: List[Concept] = []
    for item in data:
        parents = tuple(item.get("parent_codes", []))
        concepts.append(
            Concept(
                code=item["code"],
                description=item["description"],
                terminology=item["terminology"],
                active=item.get("active", True),
                parent_codes=parents,
            )
        )
    return concepts


def load_map_csv(path: str | Path) -> List[MapEntry]:
    """
    Load mapping entries from a CSV with columns:
      source_terminology, source_code, target_terminology, target_code,
      map_group (optional), map_rule (optional), map_priority (optional), map_category (optional)
    """
    path = Path(path)
    entries: List[MapEntry] = []
    with path.open(newline="", encoding="utf-8") as fh:
        reader = csv.DictReader(fh)
        for row in reader:
            entries.append(
                MapEntry(
                    source_terminology=row["source_terminology"].strip(),
                    source_code=row["source_code"].strip(),
                    target_terminology=row["target_terminology"].strip(),
                    target_code=row["target_code"].strip(),
                    map_group=int(row.get("map_group", 1)),
                    map_rule=row.get("map_rule", "").strip(),
                    map_priority=int(row.get("map_priority", 1)),
                    map_category=row.get("map_category", "").strip(),
                )
            )
    return entries


def load_map_json(path: str | Path) -> List[MapEntry]:
    """Load mapping entries from a JSON list of objects."""
    path = Path(path)
    with path.open(encoding="utf-8") as fh:
        data = json.load(fh)
    entries: List[MapEntry] = []
    for item in data:
        entries.append(
            MapEntry(
                source_terminology=item["source_terminology"],
                source_code=item["source_code"],
                target_terminology=item["target_terminology"],
                target_code=item["target_code"],
                map_group=item.get("map_group", 1),
                map_rule=item.get("map_rule", ""),
                map_priority=item.get("map_priority", 1),
                map_category=item.get("map_category", ""),
            )
        )
    return entries
