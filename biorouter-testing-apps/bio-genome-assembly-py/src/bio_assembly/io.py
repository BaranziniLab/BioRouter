"""
I/O module for reading and writing FASTA/FASTQ sequence files.

Handles both compressed and uncompressed formats, and provides
simple record-based iteration for memory-efficient processing.
"""

from __future__ import annotations

import gzip
import os
from dataclasses import dataclass
from typing import BinaryIO, Iterator, List, Optional, TextIO, Union


@dataclass
class SequenceRecord:
    """A single sequence record with identifier, description, and sequence."""
    
    id: str
    description: str
    sequence: str
    quality: Optional[str] = None  # For FASTQ files
    
    def __len__(self) -> int:
        return len(self.sequence)
    
    def __repr__(self) -> str:
        return f"SequenceRecord(id={self.id!r}, len={len(self)})"
    
    def reverse_complement(self) -> "SequenceRecord":
        """Return the reverse complement of this sequence."""
        comp = str.maketrans("ACGTacgt", "TGCAtgca")
        return SequenceRecord(
            id=self.id,
            description=self.description,
            sequence=self.sequence[::-1].translate(comp),
            quality=self.quality[::-1] if self.quality else None,
        )


def _open_file(filepath: str, mode: str = "rt") -> Union[TextIO, BinaryIO]:
    """Open a file, handling gzip compression automatically."""
    if filepath.endswith(".gz"):
        return gzip.open(filepath, mode)
    return open(filepath, mode)


def _parse_fasta_header(line: str) -> tuple[str, str]:
    """Parse a FASTA header line into (id, description)."""
    # Remove leading '>'
    header = line[1:].strip()
    parts = header.split(None, 1)
    seq_id = parts[0] if parts else ""
    description = parts[1] if len(parts) > 1 else ""
    return seq_id, description


def _parse_fastq_header(line: str) -> tuple[str, str]:
    """Parse a FASTQ header line into (id, description)."""
    # Remove leading '@'
    header = line[1:].strip()
    parts = header.split(None, 1)
    seq_id = parts[0] if parts else ""
    description = parts[1] if len(parts) > 1 else ""
    return seq_id, description


def read_fasta(filepath: str) -> Iterator[SequenceRecord]:
    """
    Read a FASTA file and yield SequenceRecord objects.
    
    Args:
        filepath: Path to FASTA file (plain or .gz compressed)
        
    Yields:
        SequenceRecord for each entry in the file
    """
    with _open_file(filepath) as f:
        current_id = None
        current_desc = ""
        current_seq: list[str] = []
        
        for line in f:
            line = line.rstrip("\n\r")
            if not line:
                continue
                
            if line.startswith(">"):
                # Yield previous record if exists
                if current_id is not None:
                    yield SequenceRecord(
                        id=current_id,
                        description=current_desc,
                        sequence="".join(current_seq),
                    )
                current_id, current_desc = _parse_fasta_header(line)
                current_seq = []
            else:
                current_seq.append(line)
        
        # Yield last record
        if current_id is not None:
            yield SequenceRecord(
                id=current_id,
                description=current_desc,
                sequence="".join(current_seq),
            )


def read_fastq(filepath: str) -> Iterator[SequenceRecord]:
    """
    Read a FASTQ file and yield SequenceRecord objects.
    
    Args:
        filepath: Path to FASTQ file (plain or .gz compressed)
        
    Yields:
        SequenceRecord for each entry in the file (with quality scores)
    """
    with _open_file(filepath) as f:
        while True:
            # Read 4 lines per record
            header_line = f.readline()
            if not header_line:
                break
            
            seq_line = f.readline()
            sep_line = f.readline()
            qual_line = f.readline()
            
            if not (seq_line and sep_line and qual_line):
                break
            
            seq_id, description = _parse_fastq_header(header_line.rstrip("\n\r"))
            sequence = seq_line.rstrip("\n\r")
            quality = qual_line.rstrip("\n\r")
            
            yield SequenceRecord(
                id=seq_id,
                description=description,
                sequence=sequence,
                quality=quality,
            )


def read_sequences(filepath: str) -> List[SequenceRecord]:
    """
    Read sequences from a file, auto-detecting FASTA vs FASTQ format.
    
    Args:
        filepath: Path to sequence file
        
    Returns:
        List of SequenceRecord objects
    """
    records = []
    
    # Peek at first character to detect format
    with _open_file(filepath) as f:
        first_char = f.read(1)
    
    if first_char == ">":
        records = list(read_fasta(filepath))
    elif first_char == "@":
        records = list(read_fastq(filepath))
    else:
        raise ValueError(f"Cannot detect format of {filepath} (first char: {first_char!r})")
    
    return records


def write_fasta(records: List[SequenceRecord], filepath: str, line_width: int = 80) -> None:
    """
    Write sequences to a FASTA file.
    
    Args:
        records: List of SequenceRecord objects
        filepath: Output file path
        line_width: Maximum line width for sequences (default: 80)
    """
    with open(filepath, "w") as f:
        for record in records:
            f.write(f">{record.id} {record.description}\n")
            seq = record.sequence
            for i in range(0, len(seq), line_width):
                f.write(seq[i:i + line_width] + "\n")


def write_fastq(records: List[SequenceRecord], filepath: str) -> None:
    """
    Write sequences to a FASTQ file.
    
    Args:
        records: List of SequenceRecord objects (must have quality scores)
        filepath: Output file path
    """
    with open(filepath, "w") as f:
        for record in records:
            if record.quality is None:
                # Generate default quality score (Q40)
                record.quality = "I" * len(record.sequence)
            f.write(f"@{record.id} {record.description}\n")
            f.write(f"{record.sequence}\n")
            f.write("+\n")
            f.write(f"{record.quality}\n")


def count_sequences(filepath: str) -> int:
    """Count the number of sequences in a file without loading them."""
    count = 0
    with _open_file(filepath) as f:
        for line in f:
            line = line.rstrip("\n\r")
            if line.startswith(">") or (line.startswith("@") and count == 0):
                count += 1
            elif line.startswith("@"):
                # FASTQ: count headers
                pass  # We count differently below
    
    # For FASTQ, we need different counting
    if filepath.endswith(".fastq") or filepath.endswith(".fq") or filepath.endswith(".fastq.gz") or filepath.endswith(".fq.gz"):
        count = 0
        with _open_file(filepath) as f:
            for i, line in enumerate(f):
                if i % 4 == 0:  # Every 4th line is a header
                    count += 1
    
    return count
