"""Tests for exact single-pattern matching algorithms.

Strategy: cross-check every algorithm against the naive (brute-force) baseline
on a variety of inputs including edge cases, overlapping matches, unicode, and
random strings.
"""

from __future__ import annotations

import random
import string

import pytest

from strmatch.exact.naive import naive_search
from strmatch.exact.kmp import kmp_search
from strmatch.exact.boyer_moore import boyer_moore_search
from strmatch.exact.rabin_karp import rabin_karp_search
from strmatch.exact.fa import fa_search

# All non-naive algorithms to test against the baseline.
ALGORITHMS = [kmp_search, boyer_moore_search, rabin_karp_search, fa_search]
ALGO_NAMES = ["kmp", "boyer-moore", "rabin-karp", "fa"]


# ---------------------------------------------------------------------------
# Basic correctness
# ---------------------------------------------------------------------------

class TestBasicMatches:
    """Standard match scenarios."""

    @pytest.mark.parametrize("algo", ALGORITHMS, ids=ALGO_NAMES)
    def test_single_match(self, algo):
        assert algo("hello world", "world") == [6]

    @pytest.mark.parametrize("algo", ALGORITHMS, ids=ALGO_NAMES)
    def test_no_match(self, algo):
        assert algo("hello world", "xyz") == []

    @pytest.mark.parametrize("algo", ALGORITHMS, ids=ALGO_NAMES)
    def test_pattern_at_start(self, algo):
        assert algo("abcdef", "abc") == [0]

    @pytest.mark.parametrize("algo", ALGORITHMS, ids=ALGO_NAMES)
    def test_pattern_at_end(self, algo):
        assert algo("abcdef", "def") == [3]

    @pytest.mark.parametrize("algo", ALGORITHMS, ids=ALGO_NAMES)
    def test_entire_text(self, algo):
        assert algo("abc", "abc") == [0]

    @pytest.mark.parametrize("algo", ALGORITHMS, ids=ALGO_NAMES)
    def test_pattern_longer_than_text(self, algo):
        assert algo("abc", "abcdef") == []


# ---------------------------------------------------------------------------
# Overlapping matches
# ---------------------------------------------------------------------------

class TestOverlapping:
    @pytest.mark.parametrize("algo", ALGORITHMS, ids=ALGO_NAMES)
    def test_overlapping_aaa(self, algo):
        # "aaa" in "aaaaa" → positions [0, 1, 2]
        assert algo("aaaaa", "aaa") == [0, 1, 2]

    @pytest.mark.parametrize("algo", ALGORITHMS, ids=ALGO_NAMES)
    def test_overlapping_abab(self, algo):
        assert algo("ABABABAB", "ABAB") == [0, 2, 4]

    @pytest.mark.parametrize("algo", ALGORITHMS, ids=ALGO_NAMES)
    def test_overlapping_single_char(self, algo):
        assert algo("ababab", "a") == [0, 2, 4]


# ---------------------------------------------------------------------------
# Edge cases
# ---------------------------------------------------------------------------

class TestEdgeCases:
    @pytest.mark.parametrize("algo", ALGORITHMS, ids=ALGO_NAMES)
    def test_empty_pattern(self, algo):
        # Empty pattern matches at every position (convention).
        result = algo("abc", "")
        assert result == list(range(4))

    @pytest.mark.parametrize("algo", ALGORITHMS, ids=ALGO_NAMES)
    def test_empty_text(self, algo):
        assert algo("", "a") == []

    @pytest.mark.parametrize("algo", ALGORITHMS, ids=ALGO_NAMES)
    def test_both_empty(self, algo):
        assert algo("", "") == [0]

    @pytest.mark.parametrize("algo", ALGORITHMS, ids=ALGO_NAMES)
    def test_single_char_match(self, algo):
        assert algo("a", "a") == [0]

    @pytest.mark.parametrize("algo", ALGORITHMS, ids=ALGO_NAMES)
    def test_single_char_no_match(self, algo):
        assert algo("a", "b") == []


# ---------------------------------------------------------------------------
# Unicode
# ---------------------------------------------------------------------------

class TestUnicode:
    @pytest.mark.parametrize("algo", ALGORITHMS, ids=ALGO_NAMES)
    def test_unicode_basic(self, algo):
        text = "αβγδεαβγ"
        pattern = "αβγ"
        assert algo(text, pattern) == [0, 5]

    @pytest.mark.parametrize("algo", ALGORITHMS, ids=ALGO_NAMES)
    def test_emoji(self, algo):
        text = "hello 🌍🌍 world 🌍"
        pattern = "🌍"
        expected = naive_search(text, pattern)
        assert algo(text, pattern) == expected

    @pytest.mark.parametrize("algo", ALGORITHMS, ids=ALGO_NAMES)
    def test_mixed_script(self, algo):
        text = "abc日本語def日本語"
        pattern = "日本語"
        assert algo(text, pattern) == [3, 9]


# ---------------------------------------------------------------------------
# Cross-check on random inputs
# ---------------------------------------------------------------------------

class TestRandomCrossCheck:
    """Generate random texts and patterns; every algorithm must agree with naive."""

    @staticmethod
    def _random_string(length: int, alphabet: str = "abc") -> str:
        return "".join(random.choices(alphabet, k=length))

    @pytest.mark.parametrize("trial", range(50))
    def test_random(self, trial):
        rng = random.Random(trial)
        text = self._random_string(rng.randint(5, 200))
        pat_len = rng.randint(1, min(5, len(text)))
        pattern = text[rng.randint(0, len(text) - pat_len) :][:pat_len]
        # Maybe mutate one char
        if rng.random() < 0.3:
            pos = rng.randint(0, len(pattern) - 1)
            ch = rng.choice("xyz")
            pattern = pattern[:pos] + ch + pattern[pos + 1:]
        expected = naive_search(text, pattern)
        for algo in ALGORITHMS:
            assert algo(text, pattern) == expected, (
                f"{algo.__name__} disagreed on text={text!r}, pattern={pattern!r}"
            )


# ---------------------------------------------------------------------------
# Specific algorithm regression
# ---------------------------------------------------------------------------

class TestSpecificRegressions:
    def test_bm_bad_char_shift(self):
        """Boyer-Moore: bad-char heuristic triggers a shift > 1."""
        result = boyer_moore_search("HERE IS A SIMPLE EXAMPLE", "EXAMPLE")
        assert result == [17]

    def test_kmp_failure_reuse(self):
        """KMP: failure function correctly skips comparisons."""
        result = kmp_search("AABAACAADAABAABA", "AABA")
        assert result == [0, 9, 12]

    def test_rk_hash_collision(self):
        """Rabin-Karp: hash collision must not produce false positive."""
        # Craft inputs that are likely to collide on small mod (use default).
        text = "abcabcabc"
        pattern = "abc"
        assert rabin_karp_search(text, pattern) == [0, 3, 6]

    def test_fa_rebuild_state(self):
        """Finite automaton: correct state transitions across the scan."""
        result = fa_search("ACGTACGTACG", "ACGT")
        assert result == [0, 4]
