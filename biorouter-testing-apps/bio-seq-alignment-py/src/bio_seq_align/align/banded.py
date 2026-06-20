"""Banded Needleman-Wunsch alignment.

Restricts the DP to a diagonal band of width 2k+1, reducing
time complexity from O(nm) to O(nk) when k << m.
"""

from __future__ import annotations

from .result import AlignmentResult
from ..matrices import get_matrix

NEG_INF = float("-inf")


def banded_alignment(
    seq1: str,
    seq2: str,
    bandwidth: int = 3,
    matrix: str | dict | None = None,
    gap_penalty: int = -2,
    match: int = 2,
    mismatch: int = -1,
) -> AlignmentResult:
    """Perform banded global alignment.

    Parameters
    ----------
    seq1, seq2 : str
    bandwidth : int
        Half-bandwidth k. The band covers 2k+1 diagonals.
        Must be >= abs(len(seq1) - len(seq2)) for valid alignment.
    matrix : str or dict, optional
    gap_penalty : int

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
    k = bandwidth

    # If the band is too narrow for the length difference, widen it
    min_k = abs(n - m)
    if k < min_k:
        k = min_k

    # We use a full matrix but only compute cells within the band
    # For memory efficiency we could use two rows, but clarity wins here
    score = [[NEG_INF] * (m + 1) for _ in range(n + 1)]
    tb = [[-1] * (m + 1) for _ in range(n + 1)]
    # 0=diag, 1=up, 2=left

    score[0][0] = 0

    # Init first column (within band)
    for i in range(1, n + 1):
        if abs(i - 0) <= k:
            score[i][0] = gap_penalty * i
            tb[i][0] = 1

    # Init first row (within band)
    for j in range(1, m + 1):
        if abs(0 - j) <= k:
            score[0][j] = gap_penalty * j
            tb[0][j] = 2

    # Fill band
    for i in range(1, n + 1):
        j_min = max(1, i - k)
        j_max = min(m, i + k)
        for j in range(j_min, j_max + 1):
            s = _subst(matrix, seq1[i - 1], seq2[j - 1])

            diag = score[i - 1][j - 1] + s if abs((i - 1) - (j - 1)) <= k else NEG_INF
            up   = score[i - 1][j] + gap_penalty if abs((i - 1) - j) <= k else NEG_INF
            left = score[i][j - 1] + gap_penalty if abs(i - (j - 1)) <= k else NEG_INF

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

    # Traceback
    a1: list[str] = []
    a2: list[str] = []
    i, j = n, m

    while i > 0 or j > 0:
        t = tb[i][j]
        if t == -1:
            # Outside band — should not happen if bandwidth is sufficient
            break
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
        score=score[n][m],
        identity=identity,
        matches=matches,
        algorithm=f"Banded-NW (k={bandwidth})",
        start1=0,
        end1=n,
        start2=0,
        end2=m,
    )


def _is_dna(seq: str) -> bool:
    return all(c in "ACGTUN-" for c in seq)


def _subst(matrix, a: str, b: str) -> int:
    try:
        return matrix[a][b]
    except (KeyError, TypeError):
        return matrix[a.upper()][b.upper()]
