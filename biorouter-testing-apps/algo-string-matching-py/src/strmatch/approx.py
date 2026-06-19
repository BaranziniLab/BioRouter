"""Approximate string matching: edit distance and k-mismatch search.

Edit distance (Levenshtein): O(n·m) time, O(min(n,m)) space (two-row DP).
k-mismatch search: O(n·m) time — reports all positions where the pattern
matches the text with at most k character substitutions (Hamming distance).
"""

from __future__ import annotations


# ---------------------------------------------------------------------------
# Edit distance (Levenshtein — insertion, deletion, substitution, each cost 1)
# ---------------------------------------------------------------------------

def edit_distance(s: str, t: str) -> int:
    """Return the Levenshtein edit distance between *s* and *t*.

    Uses the Wagner-Fischer two-row optimisation.

    Time: O(n·m).  Space: O(min(n, m)).

    >>> edit_distance("kitten", "sitting")
    3
    """
    # Make sure s is the shorter string (minimise space).
    if len(s) > len(t):
        s, t = t, s
    n, m = len(s), len(t)
    if n == 0:
        return m

    prev = list(range(n + 1))
    curr = [0] * (n + 1)

    for j in range(1, m + 1):
        curr[0] = j
        for i in range(1, n + 1):
            if s[i - 1] == t[j - 1]:
                curr[i] = prev[i - 1]
            else:
                curr[i] = 1 + min(prev[i], curr[i - 1], prev[i - 1])
        prev, curr = curr, prev

    return prev[n]


# ---------------------------------------------------------------------------
# k-mismatch search (bounded Hamming distance)
# ---------------------------------------------------------------------------

def k_mismatch_search(text: str, pattern: str, k: int) -> list[int]:
    """Return all start positions in *text* where *pattern* occurs with ≤ *k*
    character mismatches (Hamming distance, no indels).

    Time: O(n·m).  Space: O(1).

    >>> k_mismatch_search("abcdefgh", "cde", 1)
    [2, 3, 4, 5]
    """
    n, m = len(text), len(pattern)
    if m == 0:
        return list(range(n + 1))
    positions: list[int] = []
    for i in range(n - m + 1):
        mismatches = 0
        for j in range(m):
            if text[i + j] != pattern[j]:
                mismatches += 1
                if mismatches > k:
                    break
        if mismatches <= k:
            positions.append(i)
    return positions


# ---------------------------------------------------------------------------
# Fuzzy search via edit distance (bonus: all positions with ED ≤ k)
# ---------------------------------------------------------------------------

def fuzzy_search(text: str, pattern: str, max_dist: int) -> list[tuple[int, int]]:
    """Return (start_position, edit_distance) for all positions in *text*
    where a substring has Levenshtein distance ≤ *max_dist* from *pattern*.

    Uses the standard approximate-string-matching DP with free start:
    column 0 is always 0 (the match may begin at any position in the text).

    Time: O(n·m).  Space: O(m).
    """
    n, m = len(text), len(pattern)
    if m == 0:
        return [(i, 0) for i in range(n + 1)]

    prev = list(range(m + 1))
    results: list[tuple[int, int]] = []

    for i in range(1, n + 1):
        curr = [0] * (m + 1)
        curr[0] = 0  # free start: match may begin at any position
        for j in range(1, m + 1):
            if text[i - 1] == pattern[j - 1]:
                curr[j] = prev[j - 1]
            else:
                curr[j] = 1 + min(prev[j], curr[j - 1], prev[j - 1])
        if curr[m] <= max_dist:
            results.append((i - m, curr[m]))
        prev = curr

    return results
