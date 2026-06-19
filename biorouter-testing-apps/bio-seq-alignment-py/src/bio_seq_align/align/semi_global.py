"""Semi-global and overlap alignment.

Semi-global: free gaps at the start/end of one or both sequences.
Overlap: maximizes the overlap between two sequences (free gaps at
the start of seq1 and end of seq2).
"""

from __future__ import annotations

from .result import AlignmentResult
from ..matrices import get_matrix

NEG_INF = float("-inf")


def semi_global_alignment(
    seq1: str,
    seq2: str,
    matrix: str | dict | None = None,
    gap_penalty: int = -2,
    free_start1: bool = True,
    free_end1: bool = True,
    free_start2: bool = True,
    free_end2: bool = True,
    match: int = 2,
    mismatch: int = -1,
) -> AlignmentResult:
    """Semi-global alignment with configurable free-gap ends.

    By default, all four ends are free, making it a full overlap alignment.
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

    score = [[0] * (m + 1) for _ in range(n + 1)]
    tb = [[-1] * (m + 1) for _ in range(n + 1)]

    # Initialize borders with 0 (free gaps)
    for i in range(1, n + 1):
        score[i][0] = 0 if free_start2 else gap_penalty * i
        tb[i][0] = 1
    for j in range(1, m + 1):
        score[0][j] = 0 if free_start1 else gap_penalty * j
        tb[0][j] = 2

    # Fill
    for i in range(1, n + 1):
        for j in range(1, m + 1):
            s = _subst(matrix, seq1[i - 1], seq2[j - 1])
            diag = score[i - 1][j - 1] + s
            up   = score[i - 1][j] + gap_penalty
            left = score[i][j - 1] + gap_penalty

            best = diag
            t = 0
            if up > best:
                best = up
                t = 1
            if left > best:
                best = left
                t = 2
            score[i][j] = best
            tb[i][j] = t

    # Find best ending position
    best_score = NEG_INF
    bi, bj = n, m

    if free_end1 and free_end2:
        # Best score anywhere in last row or last column
        for i in range(n + 1):
            if score[i][m] > best_score:
                best_score = score[i][m]
                bi, bj = i, m
        for j in range(m + 1):
            if score[n][j] > best_score:
                best_score = score[n][j]
                bi, bj = n, j
    elif free_end1:
        for i in range(n + 1):
            if score[i][m] > best_score:
                best_score = score[i][m]
                bi, bj = i, m
    elif free_end2:
        for j in range(m + 1):
            if score[n][j] > best_score:
                best_score = score[n][j]
                bi, bj = n, j
    else:
        best_score = score[n][m]
        bi, bj = n, m

    # Traceback from (bi, bj)
    a1: list[str] = []
    a2: list[str] = []
    i, j = bi, bj

    while i > 0 and j > 0:
        t = tb[i][j]
        if t == 0:
            a1.append(seq1[i - 1])
            a2.append(seq2[j - 1])
            i -= 1; j -= 1
        elif t == 1:
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
        algorithm="Semi-global",
        start1=i,
        end1=bi,
        start2=j,
        end2=bj,
    )


def overlap_alignment(
    seq1: str,
    seq2: str,
    matrix: str | dict | None = None,
    gap_penalty: int = -2,
    match: int = 2,
    mismatch: int = -1,
) -> AlignmentResult:
    """Overlap alignment: free gaps at start of seq1 and end of seq2.

    This finds the best suffix-of-seq1 overlapping a prefix-of-seq2.
    """
    return semi_global_alignment(
        seq1, seq2,
        matrix=matrix,
        gap_penalty=gap_penalty,
        free_start1=True,   # free gaps at start of seq1
        free_end1=False,
        free_start2=False,
        free_end2=True,     # free gaps at end of seq2
        match=match,
        mismatch=mismatch,
    )


def _is_dna(seq: str) -> bool:
    return all(c in "ACGTUN-" for c in seq)


def _subst(matrix, a: str, b: str) -> int:
    try:
        return matrix[a][b]
    except (KeyError, TypeError):
        return matrix[a.upper()][b.upper()]
