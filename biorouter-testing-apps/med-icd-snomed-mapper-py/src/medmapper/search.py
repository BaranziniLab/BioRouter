"""
search.py – Fuzzy / text search over clinical concept descriptions.

Uses rapidfuzz when available, falls back to difflib for zero-dependency mode.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import List, Optional, Sequence

from .terminology import Concept, TerminologyStore

try:
    from rapidfuzz import fuzz as _fuzz  # type: ignore[import-untyped]

    def _ratio(a: str, b: str) -> float:
        return _fuzz.token_sort_ratio(a, b)

    def _partial(a: str, b: str) -> float:
        return _fuzz.partial_ratio(a, b)

    def _token_set(a: str, b: str) -> float:
        return _fuzz.token_set_ratio(a, b)

    _HAS_RAPIDFUZZ = True
except ImportError:
    import difflib

    def _ratio(a: str, b: str) -> float:  # type: ignore[misc]
        return difflib.SequenceMatcher(None, a.lower(), b.lower()).ratio() * 100

    def _partial(a: str, b: str) -> float:  # type: ignore[misc]
        # naive partial: best substring match
        a_low, b_low = a.lower(), b.lower()
        best = 0.0
        for i in range(len(a_low)):
            for j in range(i + 1, len(a_low) + 1):
                sub = a_low[i:j]
                if sub in b_low:
                    best = max(best, len(sub) / max(len(b_low), 1) * 100)
        return best

    def _token_set(a: str, b: str) -> float:  # type: ignore[misc]
        return _ratio(a, b)

    _HAS_RAPIDFUZZ = False


# ── result ───────────────────────────────────────────────────────────────────

@dataclass
class SearchResult:
    """A single search hit."""

    concept: Concept
    score: float  # 0–100, higher = better match
    match_type: str = "token_sort"  # "token_sort", "partial", "token_set", "exact"

    @property
    def code(self) -> str:
        return self.concept.code

    @property
    def description(self) -> str:
        return self.concept.description


# ── search engine ────────────────────────────────────────────────────────────

class ConceptSearch:
    """
    Fuzzy text search over concept descriptions.

    Parameters
    ----------
    store : TerminologyStore
        The concept registry to search.
    min_score : float
        Minimum score threshold (0–100).  Results below this are discarded.
    """

    def __init__(self, store: TerminologyStore, min_score: float = 40.0) -> None:
        self._store = store
        self._min_score = min_score

    def search(
        self,
        query: str,
        terminology: Optional[str] = None,
        limit: int = 10,
        match_type: str = "token_sort",
    ) -> List[SearchResult]:
        """
        Fuzzy search for concepts matching *query*.

        Parameters
        ----------
        query : str
            The search string.
        terminology : str, optional
            Restrict to a single terminology.
        limit : int
            Maximum results to return.
        match_type : str
            "token_sort" (default), "partial", or "token_set".
        """
        scorer = {
            "token_sort": _ratio,
            "partial": _partial,
            "token_set": _token_set,
        }.get(match_type, _ratio)

        concepts = (
            self._store.concepts_for(terminology)
            if terminology
            else self._store.all_concepts()
        )

        results: List[SearchResult] = []
        for concept in concepts:
            score = scorer(query, concept.description)
            if score >= self._min_score:
                results.append(SearchResult(concept=concept, score=score, match_type=match_type))

        results.sort(key=lambda r: r.score, reverse=True)
        return results[:limit]

    def search_exact(
        self, query: str, terminology: Optional[str] = None
    ) -> List[Concept]:
        """Case-insensitive exact substring match."""
        q = query.lower()
        concepts = (
            self._store.concepts_for(terminology)
            if terminology
            else self._store.all_concepts()
        )
        return [c for c in concepts if q in c.description.lower()]

    def __repr__(self) -> str:
        return f"ConceptSearch(concepts={len(self._store)}, min_score={self._min_score})"
