"""
Utility functions for bio-phylo.

Provides helpers for sequence I/O, validation, and matrix parsing.
"""

from __future__ import annotations

import re
from typing import Optional

from bio_phylo.distance import DistanceMatrix


# ======================================================================
# FASTA I/O
# ======================================================================


def parse_fasta(text: str) -> dict[str, str]:
    """Parse a FASTA-formatted string into {name: sequence}.

    Handles multi-line sequences, strips whitespace, and takes only the
    first word after '>' as the sequence name.
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
            if " " in current_name:
                current_name = current_name.split()[0]
            current_seq = []
        else:
            current_seq.append(line)
    if current_name is not None:
        sequences[current_name] = "".join(current_seq)
    return sequences


def read_fasta(path: str) -> dict[str, str]:
    """Read a FASTA file and return {name: sequence}."""
    with open(path) as f:
        return parse_fasta(f.read())


def write_fasta(sequences: dict[str, str], path: str, wrap: int = 80) -> None:
    """Write sequences to a FASTA file.

    Parameters
    ----------
    sequences : dict[str, str]
        {name: sequence}.
    path : str
        Output file path.
    wrap : int
        Line width for sequence wrapping (0 = no wrapping).
    """
    with open(path, "w") as f:
        for name, seq in sequences.items():
            f.write(f">{name}\n")
            if wrap > 0:
                for i in range(0, len(seq), wrap):
                    f.write(seq[i : i + wrap] + "\n")
            else:
                f.write(seq + "\n")


# ======================================================================
# Distance matrix parsing
# ======================================================================


def parse_distance_matrix(text: str) -> DistanceMatrix:
    """Parse a tab/whitespace-delimited distance matrix.

    Format::

        Name1 Name2 Name3 ...
        Name1  0.0    0.1   0.2
        Name2  0.1    0.0   0.3
        Name3  0.2    0.3   0.0

    The first row contains taxon names, each subsequent row starts with
    the taxon name followed by distances.
    """
    lines = [l.strip() for l in text.strip().split("\n") if l.strip()]
    if not lines:
        raise ValueError("Empty matrix")

    # First line: header with names
    header = re.split(r"\s+", lines[0])

    names = header
    values: list[list[float]] = []

    for i, line in enumerate(lines[1:], 1):
        parts = re.split(r"\s+", line.strip())
        if len(parts) < len(names):
            raise ValueError(f"Row {i} has {len(parts)} values, expected {len(names)}")
        # Skip the first element (taxon name) if present
        start = 0
        try:
            float(parts[0])
            start = 0  # No name column
        except ValueError:
            start = 1  # Name column present
        row = [float(parts[j]) for j in range(start, start + len(names))]
        values.append(row)

    return DistanceMatrix.from_square(names, values)


def read_distance_matrix(path: str) -> DistanceMatrix:
    """Read a distance matrix from a file."""
    with open(path) as f:
        return parse_distance_matrix(f.read())


# ======================================================================
# Validation helpers
# ======================================================================


def validate_alignment(sequences: dict[str, str]) -> list[str]:
    """Validate an alignment and return a list of issues.

    Checks:
    - All sequences have the same length
    - No empty sequences
    - Valid IUPAC characters (ACGTURYSWKMBDHVN-)
    """
    issues: list[str] = []
    if not sequences:
        issues.append("Alignment is empty")
        return issues

    lengths = {name: len(seq) for name, seq in sequences.items()}
    unique_lengths = set(lengths.values())
    if len(unique_lengths) > 1:
        issues.append(f"Sequences have different lengths: {unique_lengths}")

    valid_chars = set("ACGTURYSWKMBDHVNacgturyswkmbdhvn-")
    for name, seq in sequences.items():
        if not seq:
            issues.append(f"Sequence '{name}' is empty")
        invalid = set(seq) - valid_chars
        if invalid:
            issues.append(f"Sequence '{name}' has invalid characters: {invalid}")

    return issues


def alignment_summary(sequences: dict[str, str]) -> str:
    """Return a summary string of the alignment."""
    if not sequences:
        return "Empty alignment"
    names = list(sequences.keys())
    seq_len = len(sequences[names[0]])
    n_gaps = sum(seq.count("-") for seq in sequences.values())
    total_chars = len(names) * seq_len
    gap_pct = n_gaps / total_chars * 100 if total_chars > 0 else 0

    return (
        f"Alignment: {len(names)} sequences, {seq_len} positions\n"
        f"Taxa: {', '.join(names[:5])}{', ...' if len(names) > 5 else ''}\n"
        f"Gap content: {gap_pct:.1f}%"
    )
