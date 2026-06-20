"""Tests for approximate matching: edit distance and k-mismatch search."""

from __future__ import annotations

import pytest

from strmatch.approx import edit_distance, k_mismatch_search, fuzzy_search


# ---------------------------------------------------------------------------
# Edit distance (Levenshtein)
# ---------------------------------------------------------------------------

class TestEditDistance:
    def test_identical(self):
        assert edit_distance("abc", "abc") == 0

    def test_empty_vs_nonempty(self):
        assert edit_distance("", "abc") == 3
        assert edit_distance("abc", "") == 3

    def test_both_empty(self):
        assert edit_distance("", "") == 0

    def test_single_substitution(self):
        assert edit_distance("abc", "axc") == 1

    def test_single_insertion(self):
        assert edit_distance("abc", "abcd") == 1

    def test_single_deletion(self):
        assert edit_distance("abcd", "abc") == 1

    def test_classic(self):
        assert edit_distance("kitten", "sitting") == 3

    def test_classic2(self):
        assert edit_distance("saturday", "sunday") == 3

    def test_completely_different(self):
        assert edit_distance("abc", "xyz") == 3

    def test_symmetry(self):
        assert edit_distance("abc", "xyz") == edit_distance("xyz", "abc")

    def test_unicode(self):
        # One substitution: α→β
        assert edit_distance("αγγ", "βγγ") == 1

    def test_long_strings(self):
        s = "a" * 100
        t = "a" * 99 + "b"
        assert edit_distance(s, t) == 1


# ---------------------------------------------------------------------------
# k-mismatch search
# ---------------------------------------------------------------------------

class TestKMismatchSearch:
    def test_exact_match(self):
        assert k_mismatch_search("abcdef", "cde", 0) == [2]

    def test_no_match_k0(self):
        assert k_mismatch_search("abcdef", "xyz", 0) == []

    def test_one_mismatch(self):
        # "cde" in "abcdefgh" with 1 mismatch:
        # pos 2: cde vs cde → 0 mismatches ✓
        # pos 3: def vs cde → d≠c, e=e, f≠e → 2 mismatches ✗
        # Only position 2 matches with k=1.
        result = k_mismatch_search("abcdefgh", "cde", 1)
        assert result == [2]

    def test_one_mismatch_broader(self):
        # "abc" in "axcdef" with 1 mismatch → position 0 (b→x)
        result = k_mismatch_search("axcdef", "abc", 1)
        assert 0 in result

    def test_two_mismatches(self):
        # "abc" vs "xyz": 3 mismatches — not within k=2
        assert k_mismatch_search("xyzdef", "abc", 2) == []
        # "xbc" vs "abc": 1 mismatch
        assert k_mismatch_search("xbcdef", "abc", 2) == [0]

    def test_empty_pattern(self):
        assert k_mismatch_search("abc", "", 0) == [0, 1, 2, 3]

    def test_empty_text(self):
        assert k_mismatch_search("", "abc", 1) == []

    def test_k_greater_than_pattern(self):
        # Any position matches if k >= pattern length.
        result = k_mismatch_search("abc", "xyz", 3)
        assert result == [0]


# ---------------------------------------------------------------------------
# Fuzzy search (edit-distance based)
# ---------------------------------------------------------------------------

class TestFuzzySearch:
    def test_exact(self):
        # fuzzy_search with free-start: ED("cde","cde")=0 at position 2
        result = fuzzy_search("abcdef", "cde", 0)
        assert (2, 0) in result

    def test_one_edit(self):
        # "axcdef" vs "abc" with max_dist=1: ED("axc","abc")=1 at pos 0
        result = fuzzy_search("axcdef", "abc", 1)
        assert (0, 1) in result

    def test_high_threshold(self):
        result = fuzzy_search("abc", "xyz", 3)
        assert (0, 3) in result

    def test_unicode(self):
        result = fuzzy_search("αβγδ", "αγγ", 1)
        assert len(result) >= 1

    def test_no_match(self):
        result = fuzzy_search("abc", "xyz", 0)
        assert result == []
