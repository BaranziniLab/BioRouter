"""
Structure superposition and RMSD calculation.

Implements the Kabsch algorithm for optimal least-squares superposition
of two sets of paired 3-D coordinates, and RMSD computation.
"""

from __future__ import annotations

import math
from typing import List, Optional, Sequence, Tuple, TYPE_CHECKING

if TYPE_CHECKING:
    from .pdb import Chain, Model, Residue, Structure

# Type alias
Coord = Tuple[float, float, float]


# ---------------------------------------------------------------------------
# Kabsch superposition
# ---------------------------------------------------------------------------

def kabsch_superpose(
    ref: Sequence[Coord],
    mobile: Sequence[Coord],
) -> Tuple[List[Coord], float, List[List[float]]]:
    """Optimal superposition of *mobile* onto *ref* using the Kabsch algorithm.

    Parameters
    ----------
    ref : reference coordinates (N × 3)
    mobile : mobile coordinates to be rotated/translated (N × 3)

    Returns
    -------
    transformed : the transformed mobile coordinates (best-fit to ref)
    rmsd : root-mean-square deviation after superposition
    rotation : 3×3 rotation matrix

    Raises ValueError if the two coordinate sets have different lengths.
    """
    n = len(ref)
    if len(mobile) != n:
        raise ValueError(
            f"Coordinate sets must have the same length ({len(ref)} vs {len(mobile)})"
        )
    if n < 3:
        raise ValueError("Need at least 3 point pairs for superposition")

    # Step 1: Center both sets at origin
    com_ref = _centroid(ref)
    com_mobile = _centroid(mobile)

    ref_centered = [(c[0] - com_ref[0], c[1] - com_ref[1], c[2] - com_ref[2]) for c in ref]
    mob_centered = [(c[0] - com_mobile[0], c[1] - com_mobile[1], c[2] - com_mobile[2]) for c in mobile]

    # Step 2: Compute cross-covariance matrix H = mobile^T * ref
    H = [[0.0, 0.0, 0.0] for _ in range(3)]
    for i in range(n):
        for r in range(3):
            for c in range(3):
                H[r][c] += mob_centered[i][r] * ref_centered[i][c]

    # Step 3: SVD of H (3×3 only — use analytic formulas)
    U, S, Vt = _svd3(H)

    # Step 4: Ensure proper rotation (det = +1)
    d = _det3(U) * _det3(Vt)
    sign_matrix = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, d]]
    R = _mat_mul(U, _mat_mul(sign_matrix, Vt))

    # Step 5: Compute RMSD
    sse = 0.0
    for i in range(n):
        for r in range(3):
            diff = ref_centered[i][r] - sum(R[r][c] * mob_centered[i][c] for c in range(3))
            sse += diff * diff
    rmsd = math.sqrt(sse / n)

    # Step 6: Apply rotation and translate to ref centroid
    transformed = []
    for i in range(n):
        new_coord = (
            com_ref[0] + sum(R[0][c] * mob_centered[i][c] for c in range(3)),
            com_ref[1] + sum(R[1][c] * mob_centered[i][c] for c in range(3)),
            com_ref[2] + sum(R[2][c] * mob_centered[i][c] for c in range(3)),
        )
        transformed.append(new_coord)

    return transformed, rmsd, R


def _centroid(coords: Sequence[Coord]) -> Coord:
    n = len(coords)
    return (
        sum(c[0] for c in coords) / n,
        sum(c[1] for c in coords) / n,
        sum(c[2] for c in coords) / n,
    )


def _det3(m: List[List[float]]) -> float:
    """Determinant of a 3×3 matrix."""
    return (
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    )


def _mat_mul(a: List[List[float]], b: List[List[float]]) -> List[List[float]]:
    """Multiply two 3×3 matrices."""
    result = [[0.0, 0.0, 0.0] for _ in range(3)]
    for i in range(3):
        for j in range(3):
            for k in range(3):
                result[i][j] += a[i][k] * b[k][j]
    return result


def _transpose(m: List[List[float]]) -> List[List[float]]:
    return [[m[j][i] for j in range(3)] for i in range(3)]


# ---------------------------------------------------------------------------
# 3×3 SVD via Jacobi eigenvalue iteration (pure Python)
# ---------------------------------------------------------------------------

def _svd3(m: List[List[float]]) -> Tuple[List[List[float]], List[float], List[List[float]]]:
    """Compute SVD of a 3×3 matrix: m = U @ diag(S) @ Vt.

    Uses Jacobi eigenvalue iteration on m^T m, then derives U.
    Returns (U, [s0,s1,s2], Vt).
    """
    # Compute A^T A
    mt = _transpose(m)
    ata = _mat_mul(mt, m)

    # Eigendecompose ata via Jacobi
    V, evals = _jacobi3(ata)

    # Sort by eigenvalue descending
    idx = sorted(range(3), key=lambda i: -evals[i])
    evals_sorted = [evals[i] for i in idx]
    V_sorted = [[V[r][i] for i in idx] for r in range(3)]

    # Singular values
    s = [math.sqrt(max(0.0, e)) for e in evals_sorted]

    # U = m V / s
    U = [[0.0, 0.0, 0.0] for _ in range(3)]
    for j in range(3):
        if s[j] > 1e-12:
            for i in range(3):
                U[i][j] = sum(m[i][k] * V_sorted[k][j] for k in range(3)) / s[j]

    # Orthogonalize U via Gram-Schmidt if needed
    _gs3(U)

    Vt = _transpose(V_sorted)
    return U, s, Vt


def _jacobi3(m: List[List[float]]) -> Tuple[List[List[float]], List[float]]:
    """Jacobi eigenvalue algorithm for a symmetric 3×3 matrix.

    Returns (eigenvectors as columns, eigenvalues).
    """
    a = [row[:] for row in m]
    v = [[1.0 if i == j else 0.0 for j in range(3)] for i in range(3)]

    for _iteration in range(100):
        # Find largest off-diagonal
        p, q = 0, 1
        max_val = abs(a[0][1])
        for i in range(3):
            for j in range(i + 1, 3):
                if abs(a[i][j]) > max_val:
                    max_val = abs(a[i][j])
                    p, q = i, j

        if max_val < 1e-12:
            break

        # Jacobi rotation
        _jacobi_rotation(a, v, p, q)

    evals = [a[i][i] for i in range(3)]
    return v, evals


def _jacobi_rotation(
    a: List[List[float]],
    v: List[List[float]],
    p: int,
    q: int,
) -> None:
    """Apply one Jacobi rotation to eliminate a[p][q]."""
    if abs(a[p][q]) < 1e-15:
        return

    tau = (a[q][q] - a[p][p]) / (2.0 * a[p][q])
    if tau >= 0:
        t = 1.0 / (tau + math.sqrt(1.0 + tau * tau))
    else:
        t = -1.0 / (-tau + math.sqrt(1.0 + tau * tau))

    c = 1.0 / math.sqrt(1.0 + t * t)
    s = t * c

    # Update A
    ap = a[p][p]
    aq = a[q][q]
    a[p][p] = ap - t * a[p][q]
    a[q][q] = aq + t * a[p][q]
    a[p][q] = 0.0
    a[q][p] = 0.0

    for r in range(3):
        if r != p and r != q:
            arp = a[r][p]
            arq = a[r][q]
            a[r][p] = c * arp - s * arq
            a[p][r] = a[r][p]
            a[r][q] = s * arp + c * arq
            a[q][r] = a[r][q]

    # Update eigenvectors
    for r in range(3):
        vp = v[r][p]
        vq = v[r][q]
        v[r][p] = c * vp - s * vq
        v[r][q] = s * vp + c * vq


def _gs3(m: List[List[float]]) -> None:
    """Gram-Schmidt orthogonalization in-place on 3×3 columns."""
    for j in range(3):
        for jj in range(j):
            dot = sum(m[i][j] * m[i][jj] for i in range(3))
            for i in range(3):
                m[i][j] -= dot * m[i][jj]

        norm = math.sqrt(sum(m[i][j] ** 2 for i in range(3)))
        if norm > 1e-12:
            for i in range(3):
                m[i][j] /= norm


# ---------------------------------------------------------------------------
# RMSD
# ---------------------------------------------------------------------------

def rmsd(coords_a: Sequence[Coord], coords_b: Sequence[Coord]) -> float:
    """Root-mean-square deviation between two sets of paired coordinates.

    Does NOT superimpose — just measures the deviation.
    For superimposed RMSD, use ``kabsch_superposition`` first.
    """
    n = len(coords_a)
    if len(coords_b) != n:
        raise ValueError(f"Coordinate sets must have the same length ({n} vs {len(coords_b)})")
    if n == 0:
        return 0.0

    sse = 0.0
    for a, b in zip(coords_a, coords_b):
        dx = a[0] - b[0]
        dy = a[1] - b[1]
        dz = a[2] - b[2]
        sse += dx * dx + dy * dy + dz * dz
    return math.sqrt(sse / n)


def rmsd_superimposed(
    ref: Sequence[Coord],
    mobile: Sequence[Coord],
) -> float:
    """Superimpose *mobile* onto *ref* and return the RMSD."""
    _, r, _ = kabsch_superpose(ref, mobile)
    return r


# ---------------------------------------------------------------------------
# Rotation helpers
# ---------------------------------------------------------------------------

def rotate_point(
    point: Coord,
    rotation: List[List[float]],
    center: Coord = (0.0, 0.0, 0.0),
) -> Coord:
    """Rotate a point about *center* using a 3×3 rotation matrix."""
    p = (point[0] - center[0], point[1] - center[1], point[2] - center[2])
    return (
        center[0] + sum(rotation[0][c] * p[c] for c in range(3)),
        center[1] + sum(rotation[1][c] * p[c] for c in range(3)),
        center[2] + sum(rotation[2][c] * p[c] for c in range(3)),
    )


def rotation_matrix_z(angle_deg: float) -> List[List[float]]:
    """Rotation matrix about the Z axis."""
    r = math.radians(angle_deg)
    c, s = math.cos(r), math.sin(r)
    return [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]


def rotation_matrix_axis(axis: Coord, angle_deg: float) -> List[List[float]]:
    """Rotation matrix about an arbitrary unit vector *axis* by *angle_deg*.

    Uses Rodrigues' rotation formula.
    """
    ax = axis
    mag = math.sqrt(ax[0] ** 2 + ax[1] ** 2 + ax[2] ** 2)
    if mag < 1e-12:
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
    ux, uy, uz = ax[0] / mag, ax[1] / mag, ax[2] / mag

    r = math.radians(angle_deg)
    c, s = math.cos(r), math.sin(r)
    t = 1.0 - c

    return [
        [t * ux * ux + c,       t * ux * uy - s * uz,  t * ux * uz + s * uy],
        [t * ux * uy + s * uz,  t * uy * uy + c,       t * uy * uz - s * ux],
        [t * ux * uz - s * uy,  t * uy * uz + s * ux,  t * uz * uz + c],
    ]


def rotate_coords(
    coords: Sequence[Coord],
    rotation: List[List[float]],
    center: Coord = (0.0, 0.0, 0.0),
) -> List[Coord]:
    """Rotate a set of coordinates about *center*."""
    return [rotate_point(c, rotation, center) for c in coords]
