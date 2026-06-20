"""strmatch — String-matching and text-indexing library."""

from strmatch.exact import (
    naive_search,
    kmp_search,
    boyer_moore_search,
    rabin_karp_search,
    fa_search,
)
from strmatch.multi import AhoCorasick, aho_corasick_search
from strmatch.index import (
    build_suffix_array,
    build_lcp_array,
    z_algorithm,
    longest_common_substring,
    longest_repeated_substring,
)
from strmatch.approx import (
    edit_distance,
    k_mismatch_search,
)

__all__ = [
    # Exact single-pattern
    "naive_search",
    "kmp_search",
    "boyer_moore_search",
    "rabin_karp_search",
    "fa_search",
    # Multi-pattern
    "AhoCorasick",
    "aho_corasick_search",
    # Indexing
    "build_suffix_array",
    "build_lcp_array",
    "z_algorithm",
    "longest_common_substring",
    "longest_repeated_substring",
    # Approximate
    "edit_distance",
    "k_mismatch_search",
]
