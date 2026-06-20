"""Smith-Waterman local alignment with linear gap penalty."""

from __future__ import annotations

from .result import AlignmentResult
from ..matrices import BLOSUM62, get_matrix


def smith_waterman(
    seq1: str,
    seq2: str,
    matrix: str | dict | None = None,
    gap_penalty: int = -2,
    match: int = 2,
    mismatch: int = -1,
) -> AlignmentResult:
    """Perform Smith-Waterman local alignment.

    Parameters
    ----------
    seq1, seq2 : str
        Input sequences.
    matrix : str or dict, optional
        Substitution matrix name or dict.
    gap_penalty : int
        Linear gap penalty (negative). Default -2.

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

    # Score and traceback matrices
    score = [[0] * (m + 1) for _ in range(n + 1)]
    tb = [[-1] * (m + 1) for _ in range(n + 1)]
    # -1=stop, 0=diag, 1=up, 2=left

    best_score = 0
    best_i = 0
    best_j = 0

    for i in range(1, n + 1):
        for j in range(1, m + 1):
            s = _subst(matrix, seq1[i - 1], seq2[j - 1])
            diag = score[i - 1][j - 1] + s
            up   = score[i - 1][j] + gap_penalty
            left = score[i][j - 1] + gap_penalty

            best = max(0, diag, up, left)
            score[i][j] = best

            if best == 0:
                tb[i][j] = -1
            elif best == diag:
                tb[i][j] = 0
            elif best == up:
                tb[i][j] = 1
            else:
                tb[i][j] = 2

            if best > best_score:
                best_score = best
                best_i = i
                best_j = j

    # Traceback from best cell to 0
    a1: list[str] = []
    a2: list[str] = []
    i, j = best_i, best_j
    while i > 0 and j > 0 and tb[i][j] != -1:
        if tb[i][j] == 0:
            a1.append(seq1[i - 1])
            a2.append(seq2[j - 1])
            i -= 1
            j -= 1
        elif tb[i][j] == 1:
            a1.append(seq1[i - 1])
            a2.append("-")
            i -= 1
        else:
            a1.append("-")
            a2.append(seq2[j - 1])
            j -= 1

    aligned1 = "".join(reversed(a1))
    aligned2 = "".join(reversed(a2))

    matches = sum(1 for a, b in zip(aligned1, aligned2) if a == b and a != "-")
    length = len(aligned1)
    identity = matches / length if length else 0.0

    return AlignmentResult(
        aligned_seq1=aligned1,
        aligned_seq2=aligned2,
        score=best_score,
        identity=identity,
        matches=matches,
        algorithm="Smith-Waterman",
        start1=i,
        end1=best_i,
        start2=j,
        end2=best_j,
    )


def _is_dna(seq: str) -> bool:
    return all(c in "ACGTUN-" for c in seq)


def _subst(matrix, a: str, b: str) -> int:
    try:
        return matrix[a][b]
    except (KeyError, TypeError):
        return matrix[a.upper()][b.upper()]
