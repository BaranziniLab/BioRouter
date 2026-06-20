"""
Geometric calculations for atomic coordinates.

Provides functions for computing distances, bond angles, dihedral (torsion)
angles, radius of gyration, and center of mass from 3-D coordinates.

All coordinates are plain ``(x, y, z)`` tuples; no numpy required.
"""

from __future__ import annotations

import math
from typing import Sequence, Tuple

# Type alias
Coord = Tuple[float, float, float]


# ---------------------------------------------------------------------------
# Distance
# ---------------------------------------------------------------------------

def distance(a: Coord, b: Coord) -> float:
    """Euclidean distance between two points."""
    dx = a[0] - b[0]
    dy = a[1] - b[1]
    dz = a[2] - b[2]
    return math.sqrt(dx * dx + dy * dy + dz * dz)


def distance_squared(a: Coord, b: Coord) -> float:
    """Squared distance (avoids sqrt; useful for cutoffs)."""
    dx = a[0] - b[0]
    dy = a[1] - b[1]
    dz = a[2] - b[2]
    return dx * dx + dy * dy + dz * dz


# ---------------------------------------------------------------------------
# Bond angle
# ---------------------------------------------------------------------------

def bond_angle(a: Coord, b: Coord, c: Coord) -> float:
    """Angle (degrees) at vertex *b* formed by a→b→c.

    Uses the dot-product formula:
        cos(θ) = (ba · bc) / (|ba| |bc|)
    """
    ba = (a[0] - b[0], a[1] - b[1], a[2] - b[2])
    bc = (c[0] - b[0], c[1] - b[1], c[2] - b[2])

    dot = ba[0] * bc[0] + ba[1] * bc[1] + ba[2] * bc[2]
    mag_ba = math.sqrt(ba[0] ** 2 + ba[1] ** 2 + ba[2] ** 2)
    mag_bc = math.sqrt(bc[0] ** 2 + bc[1] ** 2 + bc[2] ** 2)

    if mag_ba < 1e-12 or mag_bc < 1e-12:
        return 0.0

    cos_theta = max(-1.0, min(1.0, dot / (mag_ba * mag_bc)))
    return math.degrees(math.acos(cos_theta))


# ---------------------------------------------------------------------------
# Dihedral (torsion) angle
# ---------------------------------------------------------------------------

def dihedral_angle(a: Coord, b: Coord, c: Coord, d: Coord) -> float:
    """Dihedral angle (degrees) defined by four points a→b→c→d.

    Positive = right-handed rotation about the b–c bond.

    Convention: result in [−180, +180].
    """
    # Vectors
    b1 = (b[0] - a[0], b[1] - a[1], b[2] - a[2])
    b2 = (c[0] - b[0], c[1] - b[1], c[2] - b[2])
    b3 = (d[0] - c[0], d[1] - c[1], d[2] - c[2])

    # Normal to planes
    n1 = _cross(b1, b2)
    n2 = _cross(b2, b3)

    # Unit vectors along the bond
    m1 = _cross(n1, _unit(b2))
    x = _dot(n1, n2)
    y = _dot(m1, n2)

    return math.degrees(math.atan2(y, x))


def _cross(a: Coord, b: Coord) -> Coord:
    return (
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    )


def _dot(a: Coord, b: Coord) -> float:
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def _unit(v: Coord) -> Coord:
    mag = math.sqrt(v[0] ** 2 + v[1] ** 2 + v[2] ** 2)
    if mag < 1e-12:
        return (0.0, 0.0, 0.0)
    return (v[0] / mag, v[1] / mag, v[2] / mag)


def _subtract(a: Coord, b: Coord) -> Coord:
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def _add(a: Coord, b: Coord) -> Coord:
    return (a[0] + b[0], a[1] + b[1], a[2] + b[2])


def _scale(v: Coord, s: float) -> Coord:
    return (v[0] * s, v[1] * s, v[2] * s)


def _norm(v: Coord) -> float:
    return math.sqrt(v[0] ** 2 + v[1] ** 2 + v[2] ** 2)


# ---------------------------------------------------------------------------
# Center of mass
# ---------------------------------------------------------------------------

def center_of_mass(
    coords: Sequence[Coord],
    masses: Sequence[float] | None = None,
) -> Coord:
    """Center of mass (weighted average) of a set of points.

    If *masses* is ``None`` all atoms are given equal weight (geometric center).
    """
    n = len(coords)
    if n == 0:
        raise ValueError("Need at least one coordinate")

    if masses is None:
        masses = [1.0] * n

    if len(masses) != n:
        raise ValueError("coords and masses must have the same length")

    total_mass = sum(masses)
    if total_mass < 1e-12:
        raise ValueError("Total mass is zero")

    cx = sum(c[0] * m for c, m in zip(coords, masses)) / total_mass
    cy = sum(c[1] * m for c, m in zip(coords, masses)) / total_mass
    cz = sum(c[2] * m for c, m in zip(coords, masses)) / total_mass
    return (cx, cy, cz)


# ---------------------------------------------------------------------------
# Radius of gyration
# ---------------------------------------------------------------------------

def radius_of_gyration(
    coords: Sequence[Coord],
    masses: Sequence[float] | None = None,
) -> float:
    """Radius of gyration about the center of mass.

    Rg = sqrt( Σ m_i |r_i − COM|² / Σ m_i )
    """
    com = center_of_mass(coords, masses)
    n = len(coords)
    if masses is None:
        masses = [1.0] * n
    total_mass = sum(masses)
    if total_mass < 1e-12:
        raise ValueError("Total mass is zero")

    sse = 0.0
    for c, m in zip(coords, masses):
        d2 = distance_squared(c, com)
        sse += m * d2
    return math.sqrt(sse / total_mass)


# ---------------------------------------------------------------------------
# Backbone torsion helpers (phi / psi)
# ---------------------------------------------------------------------------

def phi_angle(
    c_prev: Coord,
    n: Coord,
    ca: Coord,
    c: Coord,
) -> float | None:
    """Phi torsion: C(i-1) → N(i) → CA(i) → C(i).

    Returns None if any coordinate is missing.
    """
    if any(p is None for p in (c_prev, n, ca, c)):
        return None
    return dihedral_angle(c_prev, n, ca, c)


def psi_angle(
    n: Coord,
    ca: Coord,
    c: Coord,
    n_next: Coord,
) -> float | None:
    """Psi torsion: N(i) → CA(i) → C(i) → N(i+1).

    Returns None if any coordinate is missing.
    """
    if any(p is None for p in (n, ca, c, n_next)):
        return None
    return dihedral_angle(n, ca, c, n_next)


def omega_angle(
    ca_prev: Coord,
    c_prev: Coord,
    n: Coord,
    ca: Coord,
) -> float | None:
    """Omega torsion: CA(i-1) → C(i-1) → N(i) → CA(i).

    Returns None if any coordinate is missing.
    """
    if any(p is None for p in (ca_prev, c_prev, n, ca)):
        return None
    return dihedral_angle(ca_prev, c_prev, n, ca)
