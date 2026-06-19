"""Rabin-Karp string matching with rolling hash.

Time: O(m) preprocessing; O(n + m) expected, O(n·m) worst-case (hash collisions).
Space: O(1).
"""

_BASE = 256       # alphabet size (Unicode BMP range as proxy)
_MOD  = 1_000_000_007  # large prime modulus


def rabin_karp_search(
    text: str,
    pattern: str,
    base: int = _BASE,
    mod: int = _MOD,
) -> list[int]:
    """Return all start positions where *pattern* occurs in *text*.

    Uses Rabin-Karp with a rolling hash.  Collisions are resolved by
    character-by-character verification (Las Vegas variant).

    >>> rabin_karp_search("ABABABAB", "ABAB")
    [0, 2, 4]
    """
    n, m = len(text), len(pattern)
    if m == 0:
        return list(range(n + 1))
    if m > n:
        return []

    # Precompute base^(m-1) mod
    h = pow(base, m - 1, mod)

    # Initial hash values
    p_hash = 0
    t_hash = 0
    for i in range(m):
        p_hash = (p_hash * base + ord(pattern[i])) % mod
        t_hash = (t_hash * base + ord(text[i])) % mod

    positions: list[int] = []
    for i in range(n - m + 1):
        if p_hash == t_hash:
            # Verify (Las Vegas)
            if text[i : i + m] == pattern:
                positions.append(i)
        if i < n - m:
            t_hash = (t_hash - ord(text[i]) * h) % mod
            t_hash = (t_hash * base + ord(text[i + m])) % mod
            if t_hash < 0:
                t_hash += mod
    return positions
