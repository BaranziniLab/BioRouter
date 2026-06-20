"""FASTA file parsing and writing."""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class FastaRecord:
    """A single FASTA record."""
    id: str
    description: str
    sequence: str

    def __len__(self) -> int:
        return len(self.sequence)

    def __str__(self) -> str:
        header = f">{self.id}"
        if self.description:
            header += f" {self.description}"
        return header + "\n" + self.sequence


def parse_fasta(text: str) -> list[FastaRecord]:
    """Parse a FASTA-formatted string into a list of FastaRecord objects.

    Handles multi-line sequences and strips whitespace.
    """
    records: list[FastaRecord] = []
    current_id = ""
    current_desc = ""
    current_seq_parts: list[str] = []

    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        if line.startswith(">"):
            # save previous record
            if current_id or current_seq_parts:
                seq = "".join(current_seq_parts).replace(" ", "").upper()
                if seq:
                    records.append(FastaRecord(current_id, current_desc, seq))
            # parse header
            header = line[1:].strip()
            parts = header.split(None, 1)
            current_id = parts[0] if parts else ""
            current_desc = parts[1] if len(parts) > 1 else ""
            current_seq_parts = []
        else:
            current_seq_parts.append(line)

    # last record
    if current_id or current_seq_parts:
        seq = "".join(current_seq_parts).replace(" ", "").upper()
        if seq:
            records.append(FastaRecord(current_id, current_desc, seq))

    return records


def read_fasta(path: str | Path) -> list[FastaRecord]:
    """Read a FASTA file and return a list of FastaRecord objects."""
    p = Path(path)
    text = p.read_text()
    return parse_fasta(text)


def write_fasta(records: list[FastaRecord], path: str | Path) -> None:
    """Write records to a FASTA file."""
    p = Path(path)
    p.write_text("\n".join(str(r) for r in records) + "\n")
