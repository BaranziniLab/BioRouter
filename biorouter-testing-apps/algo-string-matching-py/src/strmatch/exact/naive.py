"""Naive (brute-force) string matching.

Time: O(n·m) worst case, where n = len(text), m = len(pattern).
Space: O(1).
"""


def naive_search(text: str, pattern: str) -> list[int]:
    """Return all start positions where *pattern* occurs in *text*.

    >>> naive_search("ABABABAB", "ABAB")
    [0, 2, 4]
    """
    n, m = len(text), len(pattern)
    if m == 0:
        return list(range(n + 1))
    positions: list[int] = []
    for i in range(n - m + 1):
        j = 0
        while j < m and text[i + j] == pattern[j]:
            j += 1
        if j == m:
            positions.append(i)
    return positions
