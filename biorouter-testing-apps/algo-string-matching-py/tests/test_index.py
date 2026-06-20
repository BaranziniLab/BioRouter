"""Tests for indexing utilities: suffix array, LCP, Z-algorithm, LCS, LRS."""

from __future__ import annotations

import random

import pytest

from strmatch.index import (
    build_suffix_array,
    build_lcp_array,
    z_algorithm,
    z_search,
    longest_common_substring,
    longest_repeated_substring,
)


# ---------------------------------------------------------------------------
# Suffix array
# ---------------------------------------------------------------------------

class TestSuffixArray:
    def test_banana(self):
        sa = build_suffix_array("banana")
        assert sa == [5, 3, 1, 0, 4, 2]

    def test_single_char(self):
        assert build_suffix_array("a") == [0]

    def test_all_same(self):
        sa = build_suffix_array("aaa")
        assert sorted(sa) == [0, 1, 2]

    def test_empty(self):
        assert build_suffix_array("") == []

    def test_sorted_suffixes(self):
        """Every consecutive pair in SA must be in lexicographic order."""
        text = "mississippi"
        sa = build_suffix_array(text)
        for i in range(len(sa) - 1):
            assert text[sa[i]:] <= text[sa[i + 1]:]

    def test_contains_all_indices(self):
        text = "abcdef"
        sa = build_suffix_array(text)
        assert sorted(sa) == list(range(len(text)))


# ---------------------------------------------------------------------------
# LCP array
# ---------------------------------------------------------------------------

class TestLCPArray:
    def test_banana(self):
        sa = build_suffix_array("banana")
        lcp = build_lcp_array("banana", sa)
        # Known LCP for "banana": [0, 1, 3, 0, 0, 2]
        assert lcp[0] == 0
        assert max(lcp) == 3

    def test_lcp_length(self):
        text = "abcdefg"
        sa = build_suffix_array(text)
        lcp = build_lcp_array(text, sa)
        assert len(lcp) == len(text)

    def test_lcp_non_negative(self):
        text = "abcabc"
        sa = build_suffix_array(text)
        lcp = build_lcp_array(text, sa)
        assert all(v >= 0 for v in lcp)


# ---------------------------------------------------------------------------
# Z-algorithm
# ---------------------------------------------------------------------------

class TestZAlgorithm:
    def test_known(self):
        z = z_algorithm("aabxaab")
        assert z == [0, 1, 0, 0, 3, 1, 0]

    def test_single_char(self):
        assert z_algorithm("a") == [0]

    def test_empty(self):
        assert z_algorithm("") == []

    def test_all_same(self):
        z = z_algorithm("aaaa")
        assert z == [0, 3, 2, 1]

    def test_no_repeats(self):
        z = z_algorithm("abcdef")
        assert z == [0, 0, 0, 0, 0, 0]


class TestZSearch:
    def test_basic(self):
        assert z_search("ABABDABACDABABCABAB", "ABABCABAB") == [10]

    def test_multiple(self):
        assert z_search("ABABABAB", "ABAB") == [0, 2, 4]

    def test_no_match(self):
        assert z_search("hello", "xyz") == []

    def test_empty_pattern(self):
        result = z_search("abc", "")
        assert result == list(range(4))


# ---------------------------------------------------------------------------
# Longest common substring
# ---------------------------------------------------------------------------

class TestLongestCommonSubstring:
    def test_known(self):
        assert longest_common_substring("banana", "ananas") == "anana"

    def test_no_common(self):
        assert longest_common_substring("abc", "xyz") == ""

    def test_identical(self):
        assert longest_common_substring("hello", "hello") == "hello"

    def test_single_char_common(self):
        result = longest_common_substring("abc", "cde")
        assert result == "c"

    def test_substring_is_longest(self):
        s = "photograph"
        t = "tomography"
        result = longest_common_substring(s, t)
        # "ograph" is common
        assert result == "ograph"


# ---------------------------------------------------------------------------
# Longest repeated substring
# ---------------------------------------------------------------------------

class TestLongestRepeatedSubstring:
    def test_banana(self):
        assert longest_repeated_substring("banana") == "ana"

    def test_no_repeats(self):
        assert longest_repeated_substring("abcdef") == ""

    def test_all_same(self):
        result = longest_repeated_substring("aaaa")
        assert result == "aaa"

    def test_single_char(self):
        assert longest_repeated_substring("a") == ""

    def test_empty(self):
        assert longest_repeated_substring("") == ""

    def test_mississippi(self):
        result = longest_repeated_substring("mississippi")
        assert result == "issi" or result == "issis" or len(result) >= 4
        # The exact answer depends on tie-breaking; verify it really repeats.
        assert result != ""
        # It must actually appear at least twice.
        idx = "mississippi".find(result)
        assert "mississippi".find(result, idx + 1) != -1
