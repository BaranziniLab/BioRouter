"""Needleman-Wunsch global alignment with linear gap penalty."""

from __future__ import annotations

from .result import AlignmentResult
from ..matrices import BLOSUM62, get_matrix


def needleman_wunsch(
    seq1: str,
    seq2: str,
    matrix: str | dict | None = None,
    gap_penalty: int = -2,
    match: int = 2,
    mismatch: int = -1,
) -> AlignmentResult:
    """Perform Needleman-Wunsch global alignment.

    Parameters
    ----------
    seq1, seq2 : str
        Input sequences.
    matrix : str or dict, optional
        Substitution matrix name or dict. Defaults to 'simple' for DNA,
        'blosum62' otherwise.
    gap_penalty : int
        Linear gap penalty (negative value). Default -2.
    match, mismatch : int
        Used only when matrix is 'simple' or not provided for DNA.

    Returns
    -------
    AlignmentResult
    """
    seq1 = seq1.upper()
    seq2 = seq2.upper()

    if matrix is None:
        matrix = "simple" if _is_dna(seq1 + seq2) else "blosum62"
    if isinstance(matrix, str):
        if matrix == "simple":
            matrix = get_matrix("simple", match=match, mismatch=mismatch)
        else:
            matrix = get_matrix(matrix)

    n = len(seq1)
    m = len(seq2)

    # Initialize score matrix
    score = [[0] * (m + 1) for _ in range(n + 1)]
    traceback = [[0] * (m + 1) for _ in range(n + 1)]
    # 0 = diag, 1 = up (gap in seq2), 2 = left (gap in seq1)

    for i in range(1, n + 1):
        score[i][0] = gap_penalty * i
        traceback[i][0] = 1
    for j in range(1, m + 1):
        score[0][j] = gap_penalty * j
        traceback[0][j] = 2

    # Fill
    for i in range(1, n + 1):
        for j in range(1, m + 1):
            s = _subst(matrix, seq1[i - 1], seq2[j - 1])
            diag = score[i - 1][j - 1] + s
            up   = score[i - 1][j] + gap_penalty
            left = score[i][j - 1] + gap_penalty

            best = diag
            tb = 0
            if up > best:
                best = up
                tb = 1
            if left > best:
                best = left
                tb = 2
            score[i][j] = best
            traceback[i][j] = tb

    # Traceback
    aligned1, aligned2 = _traceback(seq1, seq2, traceback, n, m)

    # Stats
    matches = sum(1 for a, b in zip(aligned1, aligned2) if a == b and a != "-")
    length = len(aligned1)
    identity = matches / length if length else 0.0

    return AlignmentResult(
        aligned_seq1=aligned1,
        aligned_seq2=aligned2,
        score=score[n][m],
        identity=identity,
        matches=matches,
        algorithm="Needleman-Wunsch",
        start1=0,
        end1=n,
        start2=0,
        end2=m,
    )


# ── helpers ──────────────────────────────────────────────────

def _is_dna(seq: str) -> bool:
    return all(c in "ACGTUN-" for c in seq)


def _subst(matrix, a: str, b: str) -> int:
    try:
        return matrix[a][b]
    except (KeyError, TypeError):
        return matrix[a.upper()][b.upper()]


def _traceback(seq1, seq2, tb, i, j) -> tuple[str, str]:
    a1: list[str] = []
    a2: list[str] = []
    while i > 0 or j > 0:
        if i > 0 and j > 0 and tb[i][j] == 0:
            a1.append(seq1[i - 1])
            a2.append(seq2[j - 1])
            i -= 1
            j -= 1
        elif i > 0 and tb[i][j] == 1:
            a1.append(seq1[i - 1])
            a2.append("-")
            i -= 1
        else:
            a1.append("-")
            a2.append(seq2[j - 1])
            j -= 1
    return "".join(reversed(a1)), "".join(reversed(a2))
