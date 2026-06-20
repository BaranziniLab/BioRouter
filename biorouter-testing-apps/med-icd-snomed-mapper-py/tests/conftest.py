"""Shared fixtures for the medmapper test suite."""
from __future__ import annotations

import sys
from pathlib import Path

import pytest

# Ensure src/ is importable
SRC = Path(__file__).resolve().parent.parent / "src"
if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

DATA = Path(__file__).resolve().parent.parent / "data"

from medmapper.terminology import (
    Concept,
    MapEntry,
    TerminologyStore,
    load_concepts_csv,
    load_concepts_json,
    load_map_csv,
    load_map_json,
)
from medmapper.hierarchy import Hierarchy
from medmapper.mapping import CrosswalkEngine
from medmapper.search import ConceptSearch
from medmapper.valueset import ValueSetExpander


# ── data paths ───────────────────────────────────────────────────────────────

@pytest.fixture
def icd10_csv_path() -> Path:
    return DATA / "icd10_sample.csv"


@pytest.fixture
def snomed_csv_path() -> Path:
    return DATA / "snomed_sample.csv"


@pytest.fixture
def crossmap_csv_path() -> Path:
    return DATA / "crossmap.csv"


@pytest.fixture
def concepts_json_path() -> Path:
    return DATA / "sample_concepts.json"


@pytest.fixture
def map_json_path() -> Path:
    return DATA / "sample_map.json"


# ── stores ───────────────────────────────────────────────────────────────────

@pytest.fixture
def icd10_store(icd10_csv_path: Path) -> TerminologyStore:
    store = TerminologyStore()
    store.add_many(load_concepts_csv(icd10_csv_path, "ICD-10-CM"))
    return store


@pytest.fixture
def snomed_store(snomed_csv_path: Path) -> TerminologyStore:
    store = TerminologyStore()
    store.add_many(load_concepts_csv(snomed_csv_path, "SNOMED-CT"))
    return store


@pytest.fixture
def combined_store(icd10_csv_path: Path, snomed_csv_path: Path) -> TerminologyStore:
    store = TerminologyStore()
    store.add_many(load_concepts_csv(icd10_csv_path, "ICD-10-CM"))
    store.add_many(load_concepts_csv(snomed_csv_path, "SNOMED-CT"))
    return store


@pytest.fixture
def hierarchy(combined_store: TerminologyStore) -> Hierarchy:
    return Hierarchy(combined_store)


@pytest.fixture
def engine(combined_store: TerminologyStore, crossmap_csv_path: Path) -> CrosswalkEngine:
    entries = load_map_csv(crossmap_csv_path)
    return CrosswalkEngine(combined_store, entries)


@pytest.fixture
def searcher(combined_store: TerminologyStore) -> ConceptSearch:
    return ConceptSearch(combined_store)


@pytest.fixture
def expander(combined_store: TerminologyStore, hierarchy: Hierarchy) -> ValueSetExpander:
    return ValueSetExpander(combined_store, hierarchy)


# ── tiny in-memory fixtures (for unit tests that don't need file I/O) ────────

@pytest.fixture
def tiny_store() -> TerminologyStore:
    """A minimal 5-concept store for fast unit tests."""
    store = TerminologyStore()
    store.add(Concept("D01", "Root disease", "TEST", True, ()))
    store.add(Concept("D02", "Disease A", "TEST", True, ("D01",)))
    store.add(Concept("D03", "Disease B", "TEST", True, ("D01",)))
    store.add(Concept("D04", "Sub-A1", "TEST", True, ("D02",)))
    store.add(Concept("D05", "Sub-A2", "TEST", True, ("D02",)))
    return store


@pytest.fixture
def tiny_hierarchy(tiny_store: TerminologyStore) -> Hierarchy:
    return Hierarchy(tiny_store)


@pytest.fixture
def tiny_engine(tiny_store: TerminologyStore) -> CrosswalkEngine:
    entries = [
        MapEntry("TEST", "D01", "TARGET", "T01", 1, "", 1, "equivalent"),
        MapEntry("TEST", "D02", "TARGET", "T02a", 1, "", 1, "equivalent"),
        MapEntry("TEST", "D02", "TARGET", "T02b", 1, "", 2, "narrower"),
        MapEntry("TEST", "D03", "TARGET", "T03", 1, "", 1, "equivalent"),
        MapEntry("TARGET", "T01", "TEST", "D01", 1, "", 1, "equivalent"),
        MapEntry("TARGET", "T02a", "TEST", "D02", 1, "", 1, "equivalent"),
    ]
    return CrosswalkEngine(tiny_store, entries)
