"""Knuth-Morris-Pratt (KMP) string matching.

Builds a failure function (partial match table) from the pattern.
Time: O(m) preprocessing + O(n) search = O(n + m).
Space: O(m) for the failure table.
"""


def _build_failure(pattern: str) -> list[int]:
    """Build KMP failure (partial-match) table.

    failure[i] = length of the longest proper prefix of pattern[:i+1]
    that is also a suffix.
    """
    m = len(pattern)
    failure = [0] * m
    k = 0  # length of current longest prefix-suffix
    for i in range(1, m):
        while k > 0 and pattern[k] != pattern[i]:
            k = failure[k - 1]
        if pattern[k] == pattern[i]:
            k += 1
        failure[i] = k
    return failure


def kmp_search(text: str, pattern: str) -> list[int]:
    """Return all start positions where *pattern* occurs in *text*.

    Uses the Knuth-Morris-Pratt algorithm with failure-function automaton.

    >>> kmp_search("ABABABAB", "ABAB")
    [0, 2, 4]
    """
    n, m = len(text), len(pattern)
    if m == 0:
        return list(range(n + 1))
    failure = _build_failure(pattern)
    positions: list[int] = []
    j = 0  # index into pattern
    for i in range(n):
        while j > 0 and text[i] != pattern[j]:
            j = failure[j - 1]
        if text[i] == pattern[j]:
            j += 1
        if j == m:
            positions.append(i - m + 1)
            j = failure[j - 1]
    return positions
