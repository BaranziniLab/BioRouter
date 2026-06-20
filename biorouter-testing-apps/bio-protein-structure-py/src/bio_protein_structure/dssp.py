"""
Simplified DSSP-like secondary-structure assignment.

Uses a combination of backbone hydrogen-bond geometry and phi/psi torsion
angles to assign each residue as:
  - **H**  α-helix  (3₁₀ / α / π-helix)
  - **E**  β-sheet  (extended strand)
  - **C**  coil     (everything else)

Algorithm outline:
1. For each residue *i* compute the putative backbone H-bond energy
   between C=O of residue *i* and N–H of residue *i+Δ* for Δ ∈ {−1, +1,
   +2, +3, +4, +5}.
   E = q₁q₂(1/r_ON + 1/r_CH − 1/r_OH − 1/r_CN)  (DSSP-like Coulomb).
   An H-bond is detected when E < −0.5 kcal/mol.
2. Secondary-structure patterns:
   - Helix: 4+ consecutive residues where residue *i* H-bonds to *i+3*
     (3₁₀), *i+4* (α), or *i+5* (π).
   - Sheet: 3+ consecutive residues in extended conformation with
     inter-strand H-bonds (simplified: |phi| > 90° and |psi| > 90°).
   - Coil: everything else.
3. Torsion-angle fallback: when H-bond computation is not available,
   standard Ramachandran regions are used as a proxy.

This is intentionally simplified; a production tool would need full
H-bond network analysis.
"""

from __future__ import annotations

import math
from typing import Dict, List, Optional, Tuple, TYPE_CHECKING

from .geometry import phi_angle, psi_angle, dihedral_angle

if TYPE_CHECKING:
    from .pdb import Chain, Model, Residue


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

# DSSP H-bond energy threshold (kcal/mol)
HBOND_THRESHOLD = -0.5

# Coulomb prefactor (simplified; in real DSSP this is −332)
COULOMB_CONST = -332.0

# Minimum helix length
MIN_HELIX_LEN = 3
MIN_SHEET_LEN = 3


# ---------------------------------------------------------------------------
# Backbone H-bond energy (simplified)
# ---------------------------------------------------------------------------

def _hbond_energy(
    co_o: "Coord",
    co_c: "Coord",
    nh_n: "Coord",
    nh_h: "Coord",
) -> float:
    """Simplified DSSP H-bond energy.

    E = −332 × ( 1/r_ON + 1/r_CH − 1/r_OH − 1/r_CN )
    """
    from .geometry import distance as _dist

    r_ON = _dist(nh_n, co_o)
    r_CH = _dist(nh_h, co_c)
    r_OH = _dist(nh_h, co_o)
    r_CN = _dist(nh_n, co_c)

    if any(r < 0.1 for r in (r_ON, r_CH, r_OH, r_CN)):
        return 0.0

    return COULOMB_CONST * (1.0 / r_ON + 1.0 / r_CH - 1.0 / r_OH - 1.0 / r_CN)


def _safe_coord(res: Optional["Residue"], atom_name: str) -> Optional["Coord"]:
    """Get atom coordinate or None."""
    if res is None:
        return None
    atom = res.get_atom(atom_name)
    return atom.coord if atom is not None else None


def _compute_hbond_pattern(chain: "Chain") -> Dict[int, List[int]]:
    """For each residue index, list the Δ partners (i+Δ) with E < threshold.

    Returns {res_index: [partner_indices]}.
    """
    residues = list(chain)
    n = len(residues)
    pattern: Dict[int, List[int]] = {i: [] for i in range(n)}

    for i in range(n):
        res_i = residues[i]
        o_coord = _safe_coord(res_i, "O")
        c_coord = _safe_coord(res_i, "C")

        if o_coord is None or c_coord is None:
            continue

        for delta in (-1, 1, 2, 3, 4, 5):
            j = i + delta
            if j < 0 or j >= n:
                continue
            res_j = residues[j]
            n_coord = _safe_coord(res_j, "N")
            h_coord = _safe_coord(res_j, "H")

            if n_coord is None or h_coord is None:
                continue

            energy = _hbond_energy(o_coord, c_coord, n_coord, h_coord)
            if energy < HBOND_THRESHOLD:
                pattern[i].append(j)

    return pattern


# ---------------------------------------------------------------------------
# Helix detection
# ---------------------------------------------------------------------------

def _detect_helices(
    hbonds: Dict[int, List[int]],
    n_residues: int,
) -> Dict[int, str]:
    """Detect helical residues from H-bond pattern.

    A residue is helical if it H-bonds to i+3 (3₁₀), i+4 (α), or i+5 (π)
    and is part of a continuous run of ≥ MIN_HELIX_LEN.
    """
    # Mark which residues participate in i→i+k H-bonds for k=3,4,5
    helix_mask = [False] * n_residues
    for i in range(n_residues):
        partners = hbonds.get(i, [])
        for k in (3, 4, 5):
            if i + k in partners:
                helix_mask[i] = True
                break

    # Find contiguous runs
    ss: Dict[int, str] = {}
    run_start: Optional[int] = None
    for i in range(n_residues + 1):
        if i < n_residues and helix_mask[i]:
            if run_start is None:
                run_start = i
        else:
            if run_start is not None:
                length = i - run_start
                if length >= MIN_HELIX_LEN:
                    for j in range(run_start, i):
                        ss[j] = "H"
                run_start = None

    return ss


# ---------------------------------------------------------------------------
# Sheet detection (torsion-based simplified)
# ---------------------------------------------------------------------------

def _detect_sheets(chain: "Chain") -> Dict[int, str]:
    """Detect beta-sheet residues from phi/psi torsion angles.

    Extended-strand region: |phi| ≈ 120°–180° and psi ≈ 120°–180°
    (i.e. the β-region of the Ramachandran plot).
    """
    residues = list(chain)
    n = len(residues)
    ss: Dict[int, str] = {}

    # Collect all phi/psi first
    phi_psi: List[Tuple[Optional[float], Optional[float]]] = []
    for i in range(n):
        res = residues[i]
        phi_val = None
        psi_val = None

        if i > 0:
            c_prev = _safe_coord(residues[i - 1], "C")
            n_atom = _safe_coord(res, "N")
            ca_atom = _safe_coord(res, "CA")
            c_atom = _safe_coord(res, "C")
            if all(p is not None for p in (c_prev, n_atom, ca_atom, c_atom)):
                phi_val = phi_angle(c_prev, n_atom, ca_atom, c_atom)

        if i < n - 1:
            n_atom = _safe_coord(res, "N")
            ca_atom = _safe_coord(res, "CA")
            c_atom = _safe_coord(res, "C")
            n_next = _safe_coord(residues[i + 1], "N")
            if all(p is not None for p in (n_atom, ca_atom, c_atom, n_next)):
                psi_val = psi_angle(n_atom, ca_atom, c_atom, n_next)

        phi_psi.append((phi_val, psi_val))

    # Detect extended runs
    extended_mask = [False] * n
    for i in range(n):
        phi_val, psi_val = phi_psi[i]
        if phi_val is not None and psi_val is not None:
            if abs(phi_val) > 90 and abs(psi_val) > 90:
                extended_mask[i] = True

    # Find contiguous extended runs
    run_start: Optional[int] = None
    for i in range(n + 1):
        if i < n and extended_mask[i]:
            if run_start is None:
                run_start = i
        else:
            if run_start is not None:
                length = i - run_start
                if length >= MIN_SHEET_LEN:
                    for j in range(run_start, i):
                        ss[j] = "E"
                run_start = None

    return ss


# ---------------------------------------------------------------------------
# Main assignment
# ---------------------------------------------------------------------------

def assign_secondary_structure(chain: "Chain") -> Dict[int, str]:
    """Assign secondary structure to each residue in a chain.

    Returns a dict mapping 0-based residue index to one of 'H', 'E', 'C'.
    """
    residues = list(chain)
    n = len(residues)
    if n == 0:
        return {}

    hbonds = _compute_hbond_pattern(chain)

    # Start with all coil
    ss: Dict[int, str] = {i: "C" for i in range(n)}

    # Assign helices
    helices = _detect_helices(hbonds, n)
    ss.update(helices)

    # Assign sheets
    sheets = _detect_sheets(chain)
    for idx, label in sheets.items():
        if ss.get(idx) == "C":  # Don't overwrite helix assignments
            ss[idx] = label

    return ss


def assign_structure_secondary_structure(model: "Model") -> Dict[str, Dict[int, str]]:
    """Assign secondary structure for every chain in a model.

    Returns {chain_id: {res_index: 'H'/'E'/'C'}}.
    """
    result: Dict[str, Dict[int, str]] = {}
    for chain in model:
        result[chain.chain_id] = assign_secondary_structure(chain)
    return result


# ---------------------------------------------------------------------------
# Summary statistics
# ---------------------------------------------------------------------------

def ss_summary(chain: "Chain") -> Dict[str, int]:
    """Count residues in each secondary-structure class for a chain.

    Returns {'H': n_helix, 'E': n_sheet, 'C': n_coil}.
    """
    labels = assign_secondary_structure(chain)
    summary: Dict[str, int] = {"H": 0, "E": 0, "C": 0}
    for label in labels.values():
        if label in summary:
            summary[label] += 1
    return summary


def ss_fraction(chain: "Chain") -> Dict[str, float]:
    """Fraction of residues in each secondary-structure class."""
    summary = ss_summary(chain)
    total = sum(summary.values())
    if total == 0:
        return {"H": 0.0, "E": 0.0, "C": 0.0}
    return {k: v / total for k, v in summary.items()}
