"""Benchmarking utilities for comparing string-matching algorithms."""

from __future__ import annotations

import time
from collections.abc import Callable

# Registry of exact single-pattern algorithms.
EXACT_ALGORITHMS: dict[str, Callable[[str, str], list[int]]] = {}


def _register() -> None:
    from strmatch.exact.naive import naive_search
    from strmatch.exact.kmp import kmp_search
    from strmatch.exact.boyer_moore import boyer_moore_search
    from strmatch.exact.rabin_karp import rabin_karp_search
    from strmatch.exact.fa import fa_search

    EXACT_ALGORITHMS["naive"] = naive_search
    EXACT_ALGORITHMS["kmp"] = kmp_search
    EXACT_ALGORITHMS["boyer-moore"] = boyer_moore_search
    EXACT_ALGORITHMS["rabin-karp"] = rabin_karp_search
    EXACT_ALGORITHMS["fa"] = fa_search


_register()


def get_algorithm(name: str) -> Callable[[str, str], list[int]]:
    """Look up an exact-matching algorithm by name.

    Raises ValueError if the name is unknown.
    """
    if name not in EXACT_ALGORITHMS:
        raise ValueError(
            f"Unknown algorithm {name!r}. "
            f"Available: {', '.join(EXACT_ALGORITHMS)}"
        )
    return EXACT_ALGORITHMS[name]


def time_algorithm(
    algo: Callable[[str, str], list[int]],
    text: str,
    pattern: str,
    repeats: int = 1,
) -> tuple[list[int], float]:
    """Run *algo(text, pattern)* and return (results, elapsed_seconds).

    *repeats* controls how many runs to average over.
    """
    elapsed = 0.0
    results: list[int] = []
    for _ in range(repeats):
        start = time.perf_counter()
        results = algo(text, pattern)
        elapsed += time.perf_counter() - start
    return results, elapsed / repeats


def benchmark_all(
    text: str,
    pattern: str,
    algorithms: list[str] | None = None,
    repeats: int = 3,
) -> dict[str, tuple[int, float]]:
    """Run all (or selected) algorithms and return {name: (match_count, seconds)}."""
    if algorithms is None:
        algorithms = list(EXACT_ALGORITHMS)
    results: dict[str, tuple[int, float]] = {}
    for name in algorithms:
        algo = get_algorithm(name)
        matches, elapsed = time_algorithm(algo, text, pattern, repeats=repeats)
        results[name] = (len(matches), elapsed)
    return results
