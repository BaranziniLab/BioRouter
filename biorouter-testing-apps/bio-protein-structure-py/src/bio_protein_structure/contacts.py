"""
Contact maps and clash detection.

Provides:
- Residue–residue contact maps based on Cα distance cutoffs
- Atomic clash detection using van der Waals radii
"""

from __future__ import annotations

import math
from typing import Dict, List, Optional, Set, Tuple, TYPE_CHECKING

from .geometry import distance, distance_squared

if TYPE_CHECKING:
    from .pdb import Chain, Model, Residue, Atom


# ---------------------------------------------------------------------------
# Van der Waals radii (Å) for common protein elements
# ---------------------------------------------------------------------------

VDW_RADII: Dict[str, float] = {
    "C": 1.7,
    "N": 1.55,
    "O": 1.52,
    "S": 1.8,
    "H": 1.2,
    "FE": 2.0,
    "ZN": 1.39,
    "CA": 1.98,
    "MG": 1.73,
    "P": 1.8,
}


# ---------------------------------------------------------------------------
# Contact map
# ---------------------------------------------------------------------------

def contact_map(
    chain: "Chain",
    cutoff: float = 8.0,
    ca_only: bool = True,
) -> Set[Tuple[int, int]]:
    """Compute a residue-residue contact map.

    A contact is defined as two residues whose closest atoms (or Cα atoms
    if *ca_only* is True) are within *cutoff* Å.

    Returns a set of (i, j) tuples with i < j (0-based residue indices).
    """
    residues = list(chain)
    n = len(residues)
    contacts: Set[Tuple[int, int]] = set()

    for i in range(n):
        for j in range(i + 1, n):
            if ca_only:
                ca_i = residues[i].ca
                ca_j = residues[j].ca
                if ca_i is None or ca_j is None:
                    continue
                if distance(ca_i.coord, ca_j.coord) <= cutoff:
                    contacts.add((i, j))
            else:
                min_d2 = float("inf")
                for ai in residues[i]:
                    for aj in residues[j]:
                        d2 = distance_squared(ai.coord, aj.coord)
                        if d2 < min_d2:
                            min_d2 = d2
                if math.sqrt(min_d2) <= cutoff:
                    contacts.add((i, j))

    return contacts


def contact_map_distance_matrix(chain: "Chain") -> List[List[float]]:
    """Compute pairwise Cα–Cα distance matrix.

    Returns an n×n lower-triangular-ish matrix (list of lists).
    Missing Cα atoms get float('inf').
    """
    residues = list(chain)
    n = len(residues)
    matrix: List[List[float]] = [[0.0] * n for _ in range(n)]

    for i in range(n):
        ca_i = residues[i].ca
        for j in range(i + 1, n):
            ca_j = residues[j].ca
            if ca_i is None or ca_j is None:
                d = float("inf")
            else:
                d = distance(ca_i.coord, ca_j.coord)
            matrix[i][j] = d
            matrix[j][i] = d

    return matrix


# ---------------------------------------------------------------------------
# Clash detection
# ---------------------------------------------------------------------------

def _get_vdw_radius(atom: "Atom") -> float:
    """Return the van der Waals radius for an atom, defaulting to 1.7 Å."""
    elem = atom.element.upper() if atom.element else atom.name[:1].upper()
    return VDW_RADII.get(elem, 1.7)


def clash_pairs(
    chain: "Chain",
    tolerance: float = 0.4,
    ignore_same_residue: bool = True,
) -> List[Tuple[int, int, float, float]]:
    """Find steric clashes between atoms in a chain.

    A clash occurs when two atoms are closer than
    (vdw_r1 + vdw_r2 - tolerance) Å.

    Returns list of (i, j, dist, overlap) for clashing atom pairs
    where i < j are atom serial numbers, dist is the actual distance,
    and overlap is how much they overlap.
    """
    residues = list(chain)
    atoms: List["Atom"] = []
    for res in residues:
        atoms.extend(res)

    n = len(atoms)
    clashes: List[Tuple[int, int, float, float]] = []

    for i in range(n):
        for j in range(i + 1, n):
            # Optionally skip same-residue pairs
            if ignore_same_residue:
                if (atoms[i].res_seq == atoms[j].res_seq
                        and atoms[i].chain_id == atoms[j].chain_id):
                    continue

            r1 = _get_vdw_radius(atoms[i])
            r2 = _get_vdw_radius(atoms[j])
            vdw_sum = r1 + r2 - tolerance

            d = distance(atoms[i].coord, atoms[j].coord)
            if d < vdw_sum:
                overlap = vdw_sum - d
                clashes.append((atoms[i].serial, atoms[j].serial, d, overlap))

    # Sort by overlap (most severe first)
    clashes.sort(key=lambda x: -x[3])
    return clashes


def clash_count(
    chain: "Chain",
    tolerance: float = 0.4,
) -> int:
    """Return the number of steric clashes."""
    return len(clash_pairs(chain, tolerance=tolerance))


def has_clash(chain: "Chain", tolerance: float = 0.4) -> bool:
    """Quick check: does this chain have any steric clashes?"""
    return clash_count(chain, tolerance=tolerance) > 0
