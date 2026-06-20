"""Tests for the Aho-Corasick multi-pattern matcher."""

from __future__ import annotations

import pytest

from strmatch.multi import AhoCorasick, aho_corasick_search


class TestAhoCorasick:
    def test_classic_example(self):
        """Standard textbook example."""
        ac = AhoCorasick(["he", "she", "his", "hers"])
        results = ac.search("ahishers")
        # Expected: (1,'his'), (3,'she'), (4,'he'), (4,'hers')
        assert (1, "his") in results
        assert (3, "she") in results
        assert (4, "he") in results
        assert (4, "hers") in results

    def test_single_pattern(self):
        results = aho_corasick_search("ABABABAB", ["ABAB"])
        positions = [r[0] for r in results]
        assert positions == [0, 2, 4]

    def test_overlapping_patterns(self):
        results = aho_corasick_search("aaaa", ["aa", "aaa"])
        positions = sorted(set(r[0] for r in results))
        # "aa" at 0,1,2; "aaa" at 0,1
        assert 0 in positions
        assert 1 in positions
        assert 2 in positions

    def test_no_match(self):
        results = aho_corasick_search("hello", ["xyz", "abc"])
        assert results == []

    def test_empty_patterns_list(self):
        results = aho_corasick_search("hello", [])
        assert results == []

    def test_empty_pattern_string(self):
        # Empty pattern in list — should be skipped by the automaton.
        results = aho_corasick_search("abc", [""])
        assert results == []

    def test_empty_text(self):
        results = aho_corasick_search("", ["a", "b"])
        assert results == []

    def test_pattern_equals_text(self):
        results = aho_corasick_search("abc", ["abc"])
        assert results == [(0, "abc")]

    def test_duplicate_patterns(self):
        results = aho_corasick_search("abcabc", ["abc"])
        positions = [r[0] for r in results]
        assert positions == [0, 3]

    def test_unicode_patterns(self):
        results = aho_corasick_search("αβγδεαβ", ["αβγ", "δε"])
        patterns_found = {r[1] for r in results}
        assert "αβγ" in patterns_found
        assert "δε" in patterns_found

    def test_patterns_that_are_suffixes(self):
        """Pattern B is a suffix of pattern A; both should be reported."""
        results = aho_corasick_search("abcab", ["abc", "bc"])
        patterns_at = {(r[0], r[1]) for r in results}
        assert (0, "abc") in patterns_at
        assert (1, "bc") in patterns_at

    def test_many_patterns(self):
        patterns = [f"pat{i}" for i in range(100)]
        text = "pat50 found and pat99 too"
        results = aho_corasick_search(text, patterns)
        found = {r[1] for r in results}
        assert "pat50" in found
        assert "pat99" in found

    def test_random_cross_check(self):
        """AC results must be a superset of per-pattern naive search."""
        from strmatch.exact.naive import naive_search
        import random

        rng = random.Random(42)
        alphabet = "abc"
        text = "".join(rng.choices(alphabet, k=200))
        patterns = ["".join(rng.choices(alphabet, k=rng.randint(2, 5))) for _ in range(10)]

        ac_results = aho_corasick_search(text, patterns)
        ac_set = {(pos, pat) for pos, pat in ac_results}

        for pat in patterns:
            for pos in naive_search(text, pat):
                assert (pos, pat) in ac_set, f"Missing ({pos}, {pat!r})"
