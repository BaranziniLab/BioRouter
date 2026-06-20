"""Gotoh algorithm for alignment with affine gap penalties.

Uses three matrices:
  M  – match/mismatch (main)
  Ix – gap in seq2 (insertion in seq1)
  Iy – gap in seq1 (insertion in seq2)

Gap cost = gap_open + gap_extend * (length - 1)
"""

from __future__ import annotations

from .result import AlignmentResult
from ..matrices import BLOSUM62, get_matrix

NEG_INF = float("-inf")


def gotoh_align(
    seq1: str,
    seq2: str,
    matrix: str | dict | None = None,
    gap_open: int = -5,
    gap_extend: int = -1,
    match: int = 2,
    mismatch: int = -1,
    mode: str = "global",
) -> AlignmentResult:
    """Perform Gotoh alignment with affine gap penalties.

    Parameters
    ----------
    seq1, seq2 : str
    matrix : str or dict, optional
    gap_open : int
        Penalty for opening a gap (negative). Default -5.
    gap_extend : int
        Penalty for extending a gap (negative). Default -1.
    mode : str
        'global' for Needleman-Wunsch-style, 'local' for Smith-Waterman-style.

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

    # Score matrices
    M  = [[NEG_INF] * (m + 1) for _ in range(n + 1)]
    Ix = [[NEG_INF] * (m + 1) for _ in range(n + 1)]  # gap in seq2
    Iy = [[NEG_INF] * (m + 1) for _ in range(n + 1)]  # gap in seq1

    # Traceback: 0=M diag, 1=Ix(up), 2=Iy(left) — for each matrix
    tbM  = [[-1] * (m + 1) for _ in range(n + 1)]
    tbIx = [[-1] * (m + 1) for _ in range(n + 1)]
    tbIy = [[-1] * (m + 1) for _ in range(n + 1)]

    local = mode == "local"

    # Initialize
    M[0][0] = 0
    for i in range(1, n + 1):
        if local:
            M[i][0] = 0
        else:
            M[i][0] = NEG_INF
        Ix[i][0] = gap_open + gap_extend * (i - 1) if not local else 0
        Iy[i][0] = NEG_INF
    for j in range(1, m + 1):
        if local:
            M[0][j] = 0
        else:
            M[0][j] = NEG_INF
        Ix[0][j] = NEG_INF
        Iy[0][j] = gap_open + gap_extend * (j - 1) if not local else 0

    # Fill
    best_score = 0
    best_i, best_j = 0, 0

    for i in range(1, n + 1):
        for j in range(1, m + 1):
            s = _subst(matrix, seq1[i - 1], seq2[j - 1])

            # M[i][j]: came from M (diag), Ix, or Iy
            m_diag = M[i - 1][j - 1] + s
            m_ix   = Ix[i - 1][j - 1] + s
            m_iy   = Iy[i - 1][j - 1] + s
            candidates_M = [m_diag, m_ix, m_iy]
            if local:
                candidates_M.append(0)
            M[i][j] = max(candidates_M)
            if local and M[i][j] == 0:
                tbM[i][j] = -1
            else:
                tbM[i][j] = candidates_M.index(M[i][j])

            # Ix[i][j]: gap in seq2 (extends a gap in seq1 vertically)
            ix_open   = M[i - 1][j] + gap_open + gap_extend
            ix_extend = Ix[i - 1][j] + gap_extend
            Ix[i][j] = max(ix_open, ix_extend)
            tbIx[i][j] = 0 if ix_open >= ix_extend else 1

            # Iy[i][j]: gap in seq1 (extends a gap in seq2 horizontally)
            iy_open   = M[i][j - 1] + gap_open + gap_extend
            iy_extend = Iy[i][j - 1] + gap_extend
            Iy[i][j] = max(iy_open, iy_extend)
            tbIy[i][j] = 0 if iy_open >= iy_extend else 2

            if local:
                if M[i][j] > best_score:
                    best_score = M[i][j]
                    best_i, best_j = i, j

    if local:
        final_score = best_score
        i, j = best_i, best_j
    else:
        final_score = max(M[n][m], Ix[n][m], Iy[n][m])
        if final_score == M[n][m]:
            i, j = n, m
            cur = "M"
        elif final_score == Ix[n][m]:
            i, j = n, m
            cur = "Ix"
        else:
            i, j = n, m
            cur = "Iy"

    # Traceback
    a1: list[str] = []
    a2: list[str] = []

    if local:
        cur = "M"
        while i > 0 and j > 0:
            if cur == "M":
                t = tbM[i][j]
                if t == -1:
                    break
                if t == 0:
                    a1.append(seq1[i - 1])
                    a2.append(seq2[j - 1])
                    i -= 1; j -= 1; cur = "M"
                elif t == 1:
                    a1.append(seq1[i - 1])
                    a2.append(seq2[j - 1])
                    i -= 1; j -= 1; cur = "Ix"
                else:
                    a1.append(seq1[i - 1])
                    a2.append(seq2[j - 1])
                    i -= 1; j -= 1; cur = "Iy"
            elif cur == "Ix":
                t = tbIx[i][j]
                a1.append(seq1[i - 1])
                a2.append("-")
                i -= 1
                cur = "M" if t == 0 else "Ix"
            else:  # Iy
                t = tbIy[i][j]
                a1.append("-")
                a2.append(seq2[j - 1])
                j -= 1
                cur = "M" if t == 0 else "Iy"
    else:
        cur = "M"
        if final_score == M[n][m]:
            cur = "M"
        elif final_score == Ix[n][m]:
            cur = "Ix"
        else:
            cur = "Iy"

        while i > 0 or j > 0:
            if cur == "M":
                if i == 0 and j == 0:
                    break
                t = tbM[i][j]
                if t == 0:
                    a1.append(seq1[i - 1])
                    a2.append(seq2[j - 1])
                    i -= 1; j -= 1; cur = "M"
                elif t == 1:
                    a1.append(seq1[i - 1])
                    a2.append(seq2[j - 1])
                    i -= 1; j -= 1; cur = "Ix"
                elif t == 2:
                    a1.append(seq1[i - 1])
                    a2.append(seq2[j - 1])
                    i -= 1; j -= 1; cur = "Iy"
                else:
                    break
            elif cur == "Ix":
                if i == 0:
                    break
                t = tbIx[i][j]
                a1.append(seq1[i - 1])
                a2.append("-")
                i -= 1
                cur = "M" if t == 0 else "Ix"
            else:
                if j == 0:
                    break
                t = tbIy[i][j]
                a1.append("-")
                a2.append(seq2[j - 1])
                j -= 1
                cur = "M" if t == 0 else "Iy"

    aligned1 = "".join(reversed(a1))
    aligned2 = "".join(reversed(a2))

    matches = sum(1 for a, b in zip(aligned1, aligned2) if a == b and a != "-")
    length = len(aligned1)
    identity = matches / length if length else 0.0

    return AlignmentResult(
        aligned_seq1=aligned1,
        aligned_seq2=aligned2,
        score=final_score,
        identity=identity,
        matches=matches,
        algorithm=f"Gotoh ({mode})",
        start1=i if local else 0,
        end1=best_i if local else n,
        start2=j if local else 0,
        end2=best_j if local else m,
    )


def _is_dna(seq: str) -> bool:
    return all(c in "ACGTUN-" for c in seq)


def _subst(matrix, a: str, b: str) -> int:
    try:
        return matrix[a][b]
    except (KeyError, TypeError):
        return matrix[a.upper()][b.upper()]
