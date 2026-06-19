"""Substitution matrices for sequence alignment."""

from __future__ import annotations

# ── BLOSUM62 ─────────────────────────────────────────────────
# Standard BLOSUM62 matrix (Henikoff & Henikoff 1992).
# Stored as a dict-of-dicts: BLOSUM62['A']['G'] == 1

_BLOSUM62_RAW = """\
   A  R  N  D  C  Q  E  G  H  I  L  K  M  F  P  S  T  W  Y  V  B  Z  X  *
A  4 -1 -2 -2  0 -1 -1  0 -2 -1 -1 -1 -1 -2 -1  1  0 -3 -2  0 -2 -1  0 -4
R -1  5  0 -2 -3  1  0 -2  0 -3 -2  2 -1 -3 -2 -1 -1 -3 -2 -3 -1  0 -1 -4
N -2  0  6  1 -3  0  0  0  1 -3 -3  0 -2 -3 -2  1  0 -4 -2 -3  3  0 -1 -4
D -2 -2  1  6 -3  0  2 -1 -1 -3 -4 -1 -3 -3 -1  0 -1 -4 -3 -3  4  1 -1 -4
C  0 -3 -3 -3  9 -3 -4 -3 -3 -1 -1 -3 -1 -2 -3 -1 -1 -2 -2 -1 -3 -3 -2 -4
Q -1  1  0  0 -3  5  2 -2  0 -3 -2  1  0 -3 -1  0 -1 -2 -1 -2  0  3 -1 -4
E -1  0  0  2 -4  2  5 -2  0 -3 -3  1 -2 -3 -1  0 -1 -3 -2 -2  1  4 -1 -4
G  0 -2  0 -1 -3 -2 -2  6 -2 -4 -4 -2 -3 -3 -2  0 -2 -2 -3 -3 -1 -2 -1 -4
H -2  0  1 -1 -3  0  0 -2  8 -3 -3 -1 -2 -1 -2 -1 -2 -2  2 -3  0  0 -1 -4
I -1 -3 -3 -3 -1 -3 -3 -4 -3  4  2 -3  1  0 -3 -2 -1 -3 -1  3 -3 -3 -1 -4
L -1 -2 -3 -4 -1 -2 -3 -4 -3  2  4 -2  2  0 -3 -2 -1 -2 -1  1 -4 -3 -1 -4
K -1  2  0 -1 -3  1  1 -2 -1 -3 -2  5 -1 -3 -1  0 -1 -3 -2 -2  0  1 -1 -4
M -1 -1 -2 -3 -1  0 -2 -3 -2  1  2 -1  5  0 -2 -1 -1 -1 -1  1 -3 -1 -1 -4
F -2 -3 -3 -3 -2 -3 -3 -3 -1  0  0 -3  0  6 -4 -2 -2  1  3 -1 -3 -3 -1 -4
P -1 -2 -2 -1 -3 -1 -1 -2 -2 -3 -3 -1 -2 -4  7 -1 -1 -4 -3 -2 -2 -1 -2 -4
S  1 -1  0  0 -1  0  0  0 -1 -2 -2  0 -1 -2 -1  4  1 -3 -2 -2  0  0  0 -4
T  0 -1  0 -1 -1 -1 -1 -2 -2 -1 -1 -1 -1 -2 -1  1  5 -2 -2  0 -1 -1  0 -4
W -3 -3 -4 -4 -2 -2 -3 -2 -2 -3 -2 -3 -1  1 -4 -3 -2 11  2 -3 -4 -3 -2 -4
Y -2 -2 -2 -3 -2 -1 -2 -3  2 -1 -1 -2 -1  3 -3 -2 -2  2  7 -1 -3 -2 -1 -4
V  0 -3 -3 -3 -1 -2 -2 -3 -3  3  1 -2  1 -1 -2 -2  0 -3 -1  4 -3 -2 -1 -4
B -2 -1  3  4 -3  0  1 -1  0 -3 -4  0 -3 -3 -2  0 -1 -4 -3 -3  4  1 -1 -4
Z -1  0  0  1 -3  3  4 -2  0 -3 -3  1 -1 -3 -1  0 -1 -3 -2 -2  1  4 -1 -4
X  0 -1 -1 -1 -2 -1 -1 -1 -1 -1 -1 -1 -1 -1 -2  0  0 -2 -1 -1 -1 -1 -1 -4
* -4 -4 -4 -4 -4 -4 -4 -4 -4 -4 -4 -4 -4 -4 -4 -4 -4 -4 -4 -4 -4 -4 -4  1
"""


def _parse_blosum(raw: str) -> dict[str, dict[str, int]]:
    lines = [l.strip() for l in raw.strip().splitlines() if l.strip()]
    headers = lines[0].split()
    matrix: dict[str, dict[str, int]] = {}
    for line in lines[1:]:
        parts = line.split()
        row_aa = parts[0]
        matrix[row_aa] = {}
        for j, val in enumerate(parts[1:]):
            matrix[row_aa][headers[j]] = int(val)
    return matrix


BLOSUM62: dict[str, dict[str, int]] = _parse_blosum(_BLOSUM62_RAW)


# ── Simple match / mismatch ─────────────────────────────────

class SimpleScoring:
    """A simple match (+match_score) / mismatch (+mismatch_score) scheme.

    Treats every character pair identically — useful for DNA.
    """

    def __init__(self, match: int = 2, mismatch: int = -1) -> None:
        self.match = match
        self.mismatch = mismatch

    def __getitem__(self, key: str) -> dict[str, int]:
        """Return a row-like dict for the given character."""
        aa = key.upper()
        # Return a dict-like object that scores every other char
        return _SimpleRow(aa, self.match, self.mismatch)

    def get(self, key: str, default=None):
        try:
            return self[key]
        except KeyError:
            return default


class _SimpleRow:
    __slots__ = ("_aa", "_match", "_mismatch")

    def __init__(self, aa: str, match: int, mismatch: int) -> None:
        self._aa = aa
        self._match = match
        self._mismatch = mismatch

    def __getitem__(self, other: str) -> int:
        return self._match if other.upper() == self._aa else self._mismatch

    def get(self, key: str, default=None):
        try:
            return self[key]
        except KeyError:
            return default


# ── Factory ──────────────────────────────────────────────────

def get_matrix(name: str = "blosum62", **kwargs) -> dict:
    """Return a substitution matrix by name.

    Names: 'blosum62', 'simple', 'dna', 'identity'.
    For 'simple'/'dna', optional kwargs: match (default 2), mismatch (default -1).
    """
    name = name.lower()
    if name in ("blosum62", "blosum"):
        return BLOSUM62
    if name in ("simple", "dna"):
        return SimpleScoring(match=kwargs.get("match", 2), mismatch=kwargs.get("mismatch", -1))
    if name == "identity":
        return SimpleScoring(match=1, mismatch=0)
    raise ValueError(f"Unknown matrix: {name!r}. Choose from: blosum62, simple, dna, identity")
