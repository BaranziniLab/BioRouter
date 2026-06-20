"""
Residue composition and sequence extraction.

Provides:
- 3-letter ↔ 1-letter amino acid code conversion
- Sequence extraction from Chain / Structure objects
- Residue composition counting
"""

from __future__ import annotations

from collections import Counter
from typing import Dict, List, Optional, TYPE_CHECKING

if TYPE_CHECKING:
    from .pdb import Chain, Residue, Structure


# ---------------------------------------------------------------------------
# Amino acid code tables
# ---------------------------------------------------------------------------

THREE_TO_ONE: Dict[str, str] = {
    "ALA": "A",
    "ARG": "R",
    "ASN": "N",
    "ASP": "D",
    "CYS": "C",
    "GLU": "E",
    "GLN": "Q",
    "GLY": "G",
    "HIS": "H",
    "ILE": "I",
    "LEU": "L",
    "LYS": "K",
    "MET": "M",
    "PHE": "F",
    "PRO": "P",
    "SER": "S",
    "THR": "T",
    "TRP": "W",
    "TYR": "Y",
    "VAL": "V",
    "SEC": "U",
    "PYL": "O",
    # Common non-standard that are still standard-ish
    "MSE": "M",  # selenomethionine
}

ONE_TO_THREE: Dict[str, str] = {v: k for k, v in THREE_TO_ONE.items()}

# Backward-compat alias
AA_3_TO_1 = THREE_TO_ONE
AA_1_TO_3 = ONE_TO_THREE

STANDARD_AA_1 = set(THREE_TO_ONE.values())
STANDARD_AA_3 = set(THREE_TO_ONE.keys())


def three_to_one(resname: str) -> str:
    """Convert a 3-letter residue name to 1-letter code.

    Returns ``X`` for unknown/non-standard residues.
    """
    return THREE_TO_ONE.get(resname.upper(), "X")


def one_to_three(code: str) -> str:
    """Convert a 1-letter amino acid code to 3-letter name.

    Raises ``ValueError`` for unknown codes.
    """
    code = code.upper()
    if code not in ONE_TO_THREE:
        raise ValueError(f"Unknown 1-letter amino acid code: {code!r}")
    return ONE_TO_THREE[code]


def is_standard_amino_acid(resname: str) -> bool:
    """Return True if *resname* is one of the 20 standard amino acids."""
    return resname.upper() in STANDARD_AA_3


# ---------------------------------------------------------------------------
# Sequence extraction
# ---------------------------------------------------------------------------

def chain_sequence_3letter(chain: Chain) -> List[str]:
    """Return a list of 3-letter residue names for a chain."""
    return [res.name for res in chain]


def chain_sequence_1letter(chain: Chain) -> str:
    """Return the 1-letter amino acid sequence for a chain.

    Non-standard residues become ``X``.
    """
    return "".join(three_to_one(res.name) for res in chain)


def chain_sequence_with_gap(chain: Chain, gap: str = "X") -> str:
    """Like ``chain_sequence_1letter`` but tracks residue-number gaps.

    A ``-`` is inserted whenever the residue sequence number jumps
    by more than 1 between consecutive residues.
    """
    parts: List[str] = []
    prev_seq: Optional[int] = None
    for res in chain:
        if prev_seq is not None and res.res_seq != prev_seq + 1:
            parts.append(gap)
        parts.append(three_to_one(res.name))
        prev_seq = res.res_seq
    return "".join(parts)


# ---------------------------------------------------------------------------
# Composition
# ---------------------------------------------------------------------------

def residue_composition(chain: Chain) -> Dict[str, int]:
    """Count residues by 3-letter name in a chain."""
    counts: Counter[str] = Counter()
    for res in chain:
        counts[res.name] += 1
    return dict(counts)


def residue_composition_1letter(chain: Chain) -> Dict[str, int]:
    """Count residues by 1-letter code in a chain."""
    counts: Counter[str] = Counter()
    for res in chain:
        counts[three_to_one(res.name)] += 1
    return dict(counts)


def structure_composition(structure: "Structure") -> Dict[str, int]:
    """Aggregate residue counts across all models and chains.

    Uses the first model to avoid double-counting multi-model structures.
    """
    model = structure.first_model
    if model is None:
        return {}
    counts: Counter[str] = Counter()
    for chain in model:
        for res in chain:
            counts[res.name] += 1
    return dict(counts)


def residue_fraction(chain: Chain, target: str) -> float:
    """Fraction of residues matching *target* (3-letter name) in a chain."""
    total = len(chain)
    if total == 0:
        return 0.0
    target = target.upper()
    return sum(1 for r in chain if r.name == target) / total


# ---------------------------------------------------------------------------
# Helix / sheet fraction helpers (used by CLI & DSSP)
# ---------------------------------------------------------------------------

def ss_composition(
    ss_labels: Dict[int, str],
    chain_length: int,
) -> Dict[str, float]:
    """Compute helix / sheet / coil fractions from an ss_label dict.

    *ss_labels* maps 0-based residue index → 'H', 'E', or 'C'.
    Returns dict with keys 'helix', 'sheet', 'coil' (0.0–1.0).
    """
    if chain_length == 0:
        return {"helix": 0.0, "sheet": 0.0, "coil": 0.0}

    helix = sum(1 for v in ss_labels.values() if v == "H")
    sheet = sum(1 for v in ss_labels.values() if v == "E")
    coil = sum(1 for v in ss_labels.values() if v == "C")
    n = chain_length
    return {
        "helix": helix / n,
        "sheet": sheet / n,
        "coil": coil / n,
    }
