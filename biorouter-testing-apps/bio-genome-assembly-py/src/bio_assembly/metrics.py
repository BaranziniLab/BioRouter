"""
Assembly quality metrics computation.

Provides standard bioinformatics metrics for evaluating genome assemblies:
- N50 / L50 (contig size distribution)
- Total assembly length
- Number of contigs
- Longest/shortest contig
- GC content
- Gap statistics
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import List, Sequence

from .io import SequenceRecord


@dataclass
class AssemblyStats:
    """Container for assembly statistics."""
    
    num_contigs: int
    total_length: int
    longest_contig: int
    shortest_contig: int
    n50: int
    l50: int
    gc_content: float  # As fraction (0.0 - 1.0)
    num_gaps: int
    
    def __repr__(self) -> str:
        return (
            f"AssemblyStats(\n"
            f"  contigs: {self.num_contigs},\n"
            f"  total_length: {self.total_length},\n"
            f"  longest_contig: {self.longest_contig},\n"
            f"  shortest_contig: {self.shortest_contig},\n"
            f"  N50: {self.n50},\n"
            f"  L50: {self.l50},\n"
            f"  GC_content: {self.gc_content:.2%},\n"
            f"  num_gaps: {self.num_gaps}\n"
            f")"
        )
    
    def summary(self) -> str:
        """Return a human-readable summary string."""
        lines = [
            f"Assembly Statistics:",
            f"  Number of contigs: {self.num_contigs}",
            f"  Total length:      {self.total_length:,} bp",
            f"  Longest contig:    {self.longest_contig:,} bp",
            f"  Shortest contig:   {self.shortest_contig:,} bp",
            f"  N50:               {self.n50:,} bp",
            f"  L50:               {self.l50}",
            f"  GC content:        {self.gc_content:.2%}",
            f"  Number of gaps:    {self.num_gaps}",
        ]
        return "\n".join(lines)


def compute_n50(lengths: Sequence[int]) -> tuple[int, int]:
    """
    Compute N50 and L50 statistics.
    
    N50: The contig length such that 50% of the assembly is in contigs of this size or larger.
    L50: The minimum number of contigs covering 50% of the assembly.
    
    Args:
        lengths: List of contig lengths
        
    Returns:
        Tuple of (N50, L50)
    """
    if not lengths:
        return 0, 0
    
    sorted_lengths = sorted(lengths, reverse=True)
    total = sum(sorted_lengths)
    half = total / 2
    
    cumulative = 0
    n50 = 0
    l50 = 0
    
    for length in sorted_lengths:
        cumulative += length
        l50 += 1
        if cumulative >= half:
            n50 = length
            break
    
    return n50, l50


def compute_gc_content(sequences: Sequence[str]) -> float:
    """
    Compute GC content across all sequences.
    
    Args:
        sequences: List of sequence strings
        
    Returns:
        GC content as a fraction (0.0 - 1.0)
    """
    gc_count = 0
    total_count = 0
    
    for seq in sequences:
        for base in seq.upper():
            if base in "ACGTN":
                total_count += 1
                if base in "GC":
                    gc_count += 1
    
    return gc_count / total_count if total_count > 0 else 0.0


def count_gaps(sequences: Sequence[str], gap_char: str = "N") -> int:
    """
    Count the number of gaps (runs of N's) in sequences.
    
    Args:
        sequences: List of sequence strings
        gap_char: Character to treat as gap
        
    Returns:
        Number of gap regions
    """
    count = 0
    in_gap = False
    
    for seq in sequences:
        for base in seq.upper():
            if base == gap_char:
                if not in_gap:
                    count += 1
                    in_gap = True
            else:
                in_gap = False
    
    return count


def compute_assembly_stats(sequences: Sequence[str]) -> AssemblyStats:
    """
    Compute comprehensive assembly statistics.
    
    Args:
        sequences: List of assembled contig sequences
        
    Returns:
        AssemblyStats object with all computed metrics
    """
    if not sequences:
        return AssemblyStats(
            num_contigs=0,
            total_length=0,
            longest_contig=0,
            shortest_contig=0,
            n50=0,
            l50=0,
            gc_content=0.0,
            num_gaps=0,
        )
    
    lengths = [len(seq) for seq in sequences]
    total_length = sum(lengths)
    n50, l50 = compute_n50(lengths)
    gc = compute_gc_content(sequences)
    gaps = count_gaps(sequences)
    
    return AssemblyStats(
        num_contigs=len(sequences),
        total_length=total_length,
        longest_contig=max(lengths) if lengths else 0,
        shortest_contig=min(lengths) if lengths else 0,
        n50=n50,
        l50=l50,
        gc_content=gc,
        num_gaps=gaps,
    )


def compute_assembly_stats_from_records(records: List[SequenceRecord]) -> AssemblyStats:
    """
    Compute assembly statistics from SequenceRecord objects.
    
    Args:
        records: List of SequenceRecord objects
        
    Returns:
        AssemblyStats object
    """
    return compute_assembly_stats([r.sequence for r in records])


def compare_assemblies(assembled: Sequence[str], 
                       reference: str) -> dict:
    """
    Compare assembled contigs to a reference sequence.
    
    Args:
        assembled: List of assembled contig sequences
        reference: Reference sequence string
        
    Returns:
        Dictionary with comparison metrics
    """
    # Calculate total assembled length
    assembled_length = sum(len(seq) for seq in assembled)
    ref_length = len(reference)
    
    # Calculate identity (simplified: just count matching bases in aligned regions)
    # For a real comparison, we'd need proper alignment
    assembled_concat = "".join(assembled)
    
    # Simple comparison: how much of reference is covered
    covered = 0
    for i in range(ref_length):
        if i < len(assembled_concat) and assembled_concat[i] == reference[i]:
            covered += 1
    
    identity = covered / ref_length if ref_length > 0 else 0.0
    
    return {
        "reference_length": ref_length,
        "assembled_length": assembled_length,
        "num_contigs": len(assembled),
        "identity": identity,
        "coverage": assembled_length / ref_length if ref_length > 0 else 0.0,
    }
