"""Boyer-Moore string matching (bad-character + good-suffix heuristics).

Time: O(m + σ) preprocessing; O(n·m) worst-case, sublinear average for large σ.
Space: O(m + σ).
"""


from __future__ import annotations


def _bad_char_table(pattern: str) -> dict[str, int]:
    """Map each character to its rightmost index in *pattern* (excluding last position)."""
    table: dict[str, int] = {}
    for i, ch in enumerate(pattern[:-1]):
        table[ch] = i
    return table


def _good_suffix_table(pattern: str) -> list[int]:
    """Build the good-suffix shift table.

    gs[i] = shift amount when a mismatch occurs at position i
    (0 <= i < m), using the good-suffix heuristic.
    """
    m = len(pattern)
    # suffix[i] = length of the longest suffix of pattern[:i+1] that is also
    # a suffix of pattern.  Computed right-to-left.
    suffix = [0] * m
    suffix[m - 1] = m
    g = m - 1  # rightmost position of the previous suffix match
    f = 0      # rightmost position where a different suffix match starts
    for i in range(m - 2, -1, -1):
        if i > g and suffix[i + m - 1 - f] < i - g:
            suffix[i] = suffix[i + m - 1 - f]
        else:
            if i < g:
                g = i
            f = i
            while g >= 0 and pattern[g] == pattern[g + m - 1 - f]:
                g -= 1
            suffix[i] = f - g

    # Build the good-suffix shift table.
    gs = [m] * m  # default shift = m (no good suffix matched)
    j = 0
    for i in range(m - 1, -1, -1):
        if suffix[i] == i + 1:  # prefix of pattern matches suffix
            while j < m - 1 - i:
                if gs[j] == m:
                    gs[j] = m - 1 - i
                j += 1
    for i in range(m - 1):
        gs[m - 1 - suffix[i]] = m - 1 - i
    return gs


def boyer_moore_search(text: str, pattern: str) -> list[int]:
    """Return all start positions where *pattern* occurs in *text*.

    Uses Boyer-Moore with combined bad-character and good-suffix heuristics.

    >>> boyer_moore_search("ABABABAB", "ABAB")
    [0, 2, 4]
    """
    n, m = len(text), len(pattern)
    if m == 0:
        return list(range(n + 1))
    bc = _bad_char_table(pattern)
    gs = _good_suffix_table(pattern)
    positions: list[int] = []
    skip = 0
    while skip <= n - m:
        j = m - 1
        while j >= 0 and pattern[j] == text[skip + j]:
            j -= 1
        if j < 0:
            positions.append(skip)
            skip += gs[0]
        else:
            bc_shift = j - bc.get(text[skip + j], -1)
            gs_shift = gs[j]
            skip += max(bc_shift, gs_shift)
    return positions
