"""Finite-automaton string matching.

Precomputes a transition table δ(state, char) for the pattern, then
scans the text in a single pass.

Time: O(m·|Σ|) preprocessing + O(n) search.
Space: O(m·|Σ|).
"""

from __future__ import annotations


def _build_transition_table(pattern: str) -> list[dict[str, int]]:
    """Build the DFA transition table for *pattern*.

    Returns a list of dicts:  table[state][char] = next_state.
    """
    m = len(pattern)
    alphabet: set[str] = set(pattern)

    table: list[dict[str, int]] = [{} for _ in range(m + 1)]

    for state in range(m + 1):
        for ch in alphabet:
            # Compute the longest prefix of pattern that is a suffix of
            # pattern[:state] + ch.
            candidate = pattern[:state] + ch
            k = min(m, len(candidate))
            while k > 0 and candidate[len(candidate) - k:] != pattern[:k]:
                k -= 1
            table[state][ch] = k
    return table


def fa_search(text: str, pattern: str) -> list[int]:
    """Return all start positions where *pattern* occurs in *text*.

    Uses a precomputed deterministic finite automaton.

    >>> fa_search("ABABABAB", "ABAB")
    [0, 2, 4]
    """
    n, m = len(text), len(pattern)
    if m == 0:
        return list(range(n + 1))
    table = _build_transition_table(pattern)
    positions: list[int] = []
    state = 0
    for i in range(n):
        ch = text[i]
        state = table[state].get(ch, 0)
        if state == m:
            positions.append(i - m + 1)
    return positions
