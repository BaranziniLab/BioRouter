"""Exact single-pattern matching algorithms."""

from strmatch.exact.naive import naive_search
from strmatch.exact.kmp import kmp_search
from strmatch.exact.boyer_moore import boyer_moore_search
from strmatch.exact.rabin_karp import rabin_karp_search
from strmatch.exact.fa import fa_search

__all__ = [
    "naive_search",
    "kmp_search",
    "boyer_moore_search",
    "rabin_karp_search",
    "fa_search",
]
