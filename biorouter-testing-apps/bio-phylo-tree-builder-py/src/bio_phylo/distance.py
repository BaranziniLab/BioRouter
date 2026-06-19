"""
Pairwise distance computation from aligned sequences.

Supports three substitution models:
- p-distance: proportion of differing sites
- Jukes-Cantor (JC69): single-parameter model correcting for multiple hits
- Kimura 2-parameter (K2P): two-parameter model distinguishing transitions and transversions

Also provides a DistanceMatrix class for symmetric storage and lookup.
"""

from __future__ import annotations

import math
from typing import Optional, Sequence


class DistanceMatrix:
    """Symmetric square matrix of pairwise distances indexed by taxon names.

    Internally stored as a dict-of-dicts; memory-efficient for small-medium datasets.
    """

    def __init__(self, names: Optional[list[str]] = None) -> None:
        self.names: list[str] = names or []
        self._matrix: dict[str, dict[str, float]] = {}
        for n in self.names:
            self._matrix[n] = {}

    # ------------------------------------------------------------------
    # Construction
    # ------------------------------------------------------------------

    @classmethod
    def from_dict(cls, data: dict[str, dict[str, float]]) -> DistanceMatrix:
        """Build from a nested dict {A: {B: d_AB, …}, …}.

        The matrix must be symmetric with zero diagonal.
        """
        names = list(data.keys())
        dm = cls(names)
        for n1 in names:
            for n2 in names:
                dm._matrix[n1][n2] = data[n1][n2]
        return dm

    @classmethod
    def from_square(cls, names: list[str], values: list[list[float]]) -> DistanceMatrix:
        """Build from a list-of-lists square matrix.

        values[i][j] is the distance between names[i] and names[j].
        """
        if len(names) != len(values):
            raise ValueError("names and matrix dimension mismatch")
        dm = cls(names)
        for i, n1 in enumerate(names):
            for j, n2 in enumerate(names):
                dm._matrix[n1][n2] = values[i][j]
        return dm

    # ------------------------------------------------------------------
    # Lookup
    # ------------------------------------------------------------------

    def __getitem__(self, key: tuple[str, str]) -> float:
        a, b = key
        return self._matrix[a][b]

    def __setitem__(self, key: tuple[str, str], value: float) -> None:
        a, b = key
        self._matrix[a][b] = value
        self._matrix[b][a] = value

    def get(self, a: str, b: str, default: float = 0.0) -> float:
        return self._matrix.get(a, {}).get(b, default)

    def __contains__(self, key: tuple[str, str]) -> bool:
        a, b = key
        return a in self._matrix and b in self._matrix[a]

    def __len__(self) -> int:
        return len(self.names)

    # ------------------------------------------------------------------
    # Iteration
    # ------------------------------------------------------------------

    def items(self):
        """Yield (name_i, name_j, distance) for all upper-triangle pairs."""
        for i, n1 in enumerate(self.names):
            for j, n2 in enumerate(self.names):
                if i < j:
                    yield n1, n2, self._matrix[n1][n2]

    def to_square(self) -> list[list[float]]:
        """Return a list-of-lists representation."""
        return [[self._matrix[a][b] for b in self.names] for a in self.names]

    def to_dict(self) -> dict[str, dict[str, float]]:
        """Return a nested-dict copy."""
        return {n: dict(self._matrix[n]) for n in self.names}

    # ------------------------------------------------------------------
    # Display
    # ------------------------------------------------------------------

    def __repr__(self) -> str:
        return f"DistanceMatrix({len(self.names)} taxa)"

    def formatted(self, width: int = 10, precision: int = 4) -> str:
        """Return a nicely formatted table string."""
        header = f"{'':>{width}}" + "".join(f"{n:>{width}}" for n in self.names)
        lines = [header]
        for n1 in self.names:
            row = f"{n1:>{width}}"
            for n2 in self.names:
                val = self._matrix[n1][n2]
                row += f"{val:>{width}.{precision}f}"
            lines.append(row)
        return "\n".join(lines)


# ======================================================================
# Distance models
# ======================================================================


def p_distance(seq1: str, seq2: str, gap_mode: str = "ignore") -> float:
    """Compute the p-distance (proportion of differing sites).

    Parameters
    ----------
    seq1, seq2 : str
        Aligned sequences of equal length.
    gap_mode : str
        'ignore' – sites where either sequence has a gap are excluded.
        'treat'  – gaps are treated as a fifth state.

    Returns
    -------
    float
        Proportion of differing sites (0.0 if identical).
    """
    if len(seq1) != len(seq2):
        raise ValueError(f"Sequences have different lengths: {len(seq1)} vs {len(seq2)}")
    if len(seq1) == 0:
        raise ValueError("Empty sequences")

    valid = 0
    diffs = 0
    for a, b in zip(seq1.upper(), seq2.upper()):
        if gap_mode == "ignore" and (a == "-" or b == "-"):
            continue
        valid += 1
        if a != b:
            diffs += 1
    if valid == 0:
        return 0.0
    return diffs / valid


def jukes_cantor(seq1: str, seq2: str, gap_mode: str = "ignore") -> float:
    """Compute the Jukes-Cantor (1969) evolutionary distance.

    d_JC = -3/4 * ln(1 - 4/3 * p)

    where p is the p-distance.

    Returns
    -------
    float
        Estimated number of substitutions per site.
        Returns ``float('inf')`` if the p-distance >= 0.75 (saturation).
    """
    p = p_distance(seq1, seq2, gap_mode=gap_mode)
    if p >= 0.75:
        return float("inf")
    return -0.75 * math.log(1.0 - (4.0 / 3.0) * p)


def kimura_2param(seq1: str, seq2: str, gap_mode: str = "ignore") -> float:
    """Compute the Kimura 2-parameter (1980) evolutionary distance.

    d_K2P = -1/2 ln(1 - 2P - Q) - 1/4 ln(1 - 2Q)

    where P = proportion of transitions, Q = proportion of transversions.

    Returns
    -------
    float
        Estimated number of substitutions per site.
        Returns ``float('inf')`` if the argument to any log is <= 0.
    """
    if len(seq1) != len(seq2):
        raise ValueError(f"Sequences have different lengths: {len(seq1)} vs {len(seq2)}")
    if len(seq1) == 0:
        raise ValueError("Empty sequences")

    purines = set("AG")
    pyrimidines = set("CTU")

    transitions = 0
    transversions = 0
    valid = 0

    for a, b in zip(seq1.upper(), seq2.upper()):
        if gap_mode == "ignore" and (a == "-" or b == "-"):
            continue
        if a == b:
            valid += 1
            continue
        valid += 1
        # Determine if transition or transversion
        a_is_purine = a in purines
        b_is_purine = b in purines
        if a_is_purine == b_is_purine:
            # Both purines or both pyrimidines → transition
            transitions += 1
        else:
            transversions += 1

    if valid == 0:
        return 0.0

    P = transitions / valid  # proportion of transitions
    Q = transversions / valid  # proportion of transversions

    arg1 = 1.0 - 2.0 * P - Q
    arg2 = 1.0 - 2.0 * Q

    if arg1 <= 0 or arg2 <= 0:
        return float("inf")

    return -0.5 * math.log(arg1) - 0.25 * math.log(arg2)


# ======================================================================
# Distance matrix from alignment
# ======================================================================


def compute_distance_matrix(
    sequences: dict[str, str],
    model: str = "p-distance",
    gap_mode: str = "ignore",
) -> DistanceMatrix:
    """Compute a pairwise distance matrix from an alignment.

    Parameters
    ----------
    sequences : dict[str, str]
        Mapping of taxon name → aligned sequence string.
    model : str
        One of 'p-distance', 'jukes-cantor', 'kimura-2param'.
    gap_mode : str
        'ignore' or 'treat'.

    Returns
    -------
    DistanceMatrix
    """
    model_fn = {
        "p-distance": p_distance,
        "jukes-cantor": jukes_cantor,
        "kimura-2param": kimura_2param,
        "p": p_distance,
        "jc": jukes_cantor,
        "k2p": kimura_2param,
    }
    if model not in model_fn:
        raise ValueError(f"Unknown model '{model}'. Choose from: {list(model_fn.keys())}")
    fn = model_fn[model]

    names = list(sequences.keys())
    dm = DistanceMatrix(names)
    for i, n1 in enumerate(names):
        dm._matrix[n1][n1] = 0.0
        for j in range(i + 1, len(names)):
            n2 = names[j]
            d = fn(sequences[n1], sequences[n2], gap_mode=gap_mode)
            dm._matrix[n1][n2] = d
            dm._matrix[n2][n1] = d
    return dm


def parse_fasta(text: str) -> dict[str, str]:
    """Parse a FASTA-formatted string into {name: sequence}.

    Handles multi-line sequences and strips whitespace from sequence lines.
    """
    sequences: dict[str, str] = {}
    current_name: Optional[str] = None
    current_seq: list[str] = []

    for line in text.strip().split("\n"):
        line = line.strip()
        if not line:
            continue
        if line.startswith(">"):
            if current_name is not None:
                sequences[current_name] = "".join(current_seq)
            current_name = line[1:].strip()
            # Take only the first word (before any whitespace) as the name
            if " " in current_name:
                current_name = current_name.split()[0]
            current_seq = []
        else:
            current_seq.append(line)
    if current_name is not None:
        sequences[current_name] = "".join(current_seq)
    return sequences


def read_fasta_file(path: str) -> dict[str, str]:
    """Read a FASTA file and return {name: sequence}."""
    with open(path) as f:
        return parse_fasta(f.read())
