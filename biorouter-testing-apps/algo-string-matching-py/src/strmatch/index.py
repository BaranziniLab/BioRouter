"""Text-indexing utilities: suffix array, LCP, Z-algorithm, and derived queries.

Suffix array construction: O(n log²n) via Python's built-in sort (O(n log n)
with SA-IS or similar, but Python's Timsort is fast in practice).
LCP (Kasai): O(n).
Z-algorithm: O(n).
"""

from __future__ import annotations


# ---------------------------------------------------------------------------
# Suffix array
# ---------------------------------------------------------------------------

def build_suffix_array(text: str) -> list[int]:
    """Return the suffix array of *text* (list of starting indices, sorted).

    Uses the prefix-doubling approach with Python's stable sort.

    >>> build_suffix_array("banana")
    [5, 3, 1, 0, 4, 2]
    """
    n = len(text)
    # Initial rank = ordinal of each character.
    rank = [ord(c) for c in text]
    sa = list(range(n))
    tmp = [0] * n
    k = 1
    while k < n:
        # Sort by (rank[i], rank[i+k])
        def _key(i: int) -> tuple[int, int]:
            return (rank[i], rank[i + k] if i + k < n else -1)

        sa.sort(key=_key)

        # Re-assign ranks
        tmp[sa[0]] = 0
        for i in range(1, n):
            tmp[sa[i]] = tmp[sa[i - 1]] + (1 if _key(sa[i]) != _key(sa[i - 1]) else 0)
        rank = tmp[:]
        if rank[sa[-1]] == n - 1:
            break
        k *= 2
    return sa


# ---------------------------------------------------------------------------
# LCP array (Kasai algorithm)
# ---------------------------------------------------------------------------

def build_lcp_array(text: str, sa: list[int] | None = None) -> list[int]:
    """Return the LCP array for *text* and its suffix array.

    lcp[i] = longest common prefix between suffix sa[i] and sa[i-1] (lcp[0]=0).
    Uses Kasai's algorithm in O(n).

    >>> build_lcp_array("banana", build_suffix_array("banana"))
    [0, 1, 3, 0, 0, 2]
    """
    if sa is None:
        sa = build_suffix_array(text)
    n = len(text)
    rank = [0] * n
    for i, s in enumerate(sa):
        rank[s] = i
    lcp = [0] * n
    k = 0
    for i in range(n):
        if rank[i] == 0:
            k = 0
            continue
        j = sa[rank[i] - 1]
        while i + k < n and j + k < n and text[i + k] == text[j + k]:
            k += 1
        lcp[rank[i]] = k
        if k:
            k -= 1
    return lcp


# ---------------------------------------------------------------------------
# Z-algorithm
# ---------------------------------------------------------------------------

def z_algorithm(text: str) -> list[int]:
    """Compute the Z-array of *text*.

    Z[i] = length of the longest substring starting at i that is also a
    prefix of text.  Z[0] is defined as 0 (or n by some conventions;
    we use 0).

    Time: O(n).

    >>> z_algorithm("aabxaab")
    [0, 1, 0, 0, 3, 1, 0]
    """
    n = len(text)
    z = [0] * n
    l, r = 0, 0
    for i in range(1, n):
        if i < r:
            z[i] = min(r - i, z[i - l])
        while i + z[i] < n and text[z[i]] == text[i + z[i]]:
            z[i] += 1
        if i + z[i] > r:
            l, r = i, i + z[i]
    return z


def z_search(text: str, pattern: str) -> list[int]:
    """Find all occurrences of *pattern* in *text* using the Z-algorithm.

    Constructs text' = pattern + '$' + text, computes Z-array, and reports
    positions where Z[i] == len(pattern).

    Time: O(n + m).
    """
    if not pattern:
        return list(range(len(text) + 1))
    concat = pattern + "\x00" + text  # \x00 as separator (assumed not in inputs)
    z = z_algorithm(concat)
    m = len(pattern)
    return [i - m - 1 for i in range(m + 1, len(concat)) if z[i] == m]


# ---------------------------------------------------------------------------
# Derived queries
# ---------------------------------------------------------------------------

def longest_common_substring(s: str, t: str) -> str:
    """Return the longest common substring of *s* and *t* via suffix array + LCP.

    Concatenates s + '#' + t, builds SA + LCP, then scans for the maximum LCP
    span that straddles the boundary.

    Time: O((n+m) log(n+m))  (dominated by SA construction).

    >>> longest_common_substring("banana", "ananas")
    'anana'
    """
    sep = "\x00"
    combined = s + sep + t
    sa = build_suffix_array(combined)
    lcp = build_lcp_array(combined, sa)
    n_s = len(s)
    best_len = 0
    best_start = 0
    for i in range(1, len(combined)):
        a, b = sa[i - 1], sa[i]
        # Must straddle the separator.
        on_different_sides = (a < n_s) != (b < n_s)
        if on_different_sides and lcp[i] > best_len:
            best_len = lcp[i]
            best_start = sa[i] if sa[i] < n_s else sa[i - 1]
    return s[best_start : best_start + best_len]


def longest_repeated_substring(text: str) -> str:
    """Return the longest repeated substring in *text* via suffix array + LCP.

    The answer is the longest span in the LCP array (the maximum LCP value
    gives the length; the starting position comes from the corresponding SA
    entry).

    Time: O(n log n).

    >>> longest_repeated_substring("banana")
    'ana'
    """
    if not text:
        return ""
    sa = build_suffix_array(text)
    lcp = build_lcp_array(text, sa)
    max_idx = 0
    for i in range(1, len(lcp)):
        if lcp[i] > lcp[max_idx]:
            max_idx = i
    return text[sa[max_idx] : sa[max_idx] + lcp[max_idx]] if lcp[max_idx] > 0 else ""
