"""
Overlap detection module for suffix-prefix overlaps between sequences.

Implements efficient overlap detection using:
- Suffix/prefix matching with configurable minimum overlap length
- Error tolerance using Hamming distance
- Suffix array optimization for large datasets

Used by the OLC (Overlap-Layout-Consensus) assembler.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Dict, List, Optional, Set, Tuple

from .io import SequenceRecord


@dataclass
class Overlap:
    """Represents a suffix-prefix overlap between two sequences."""
    
    read_a: int          # Index of first read
    read_b: int          # Index of second read
    offset: int          # Start position in read_a where read_b begins
    length: int          # Length of overlap
    score: float         # Similarity score (0.0-1.0)
    is_reverse: bool     # If True, read_b is reverse-complemented
    
    @property
    def end_a(self) -> int:
        """End position of overlap in read_a (exclusive)."""
        return self.offset + self.length
    
    @property
    def gap(self) -> int:
        """Gap between reads (positive = overlap, negative = gap)."""
        return -self.length  # Negative means overlap
    
    def __repr__(self) -> str:
        rev = " (rev)" if self.is_reverse else ""
        return (f"Overlap(a={self.read_a}, b={self.read_b}, "
                f"offset={self.offset}, len={self.length}, "
                f"score={self.score:.3f}{rev})")


def hamming_distance(s1: str, s2: str) -> int:
    """Compute Hamming distance between two equal-length strings."""
    if len(s1) != len(s2):
        raise ValueError("Strings must be equal length")
    return sum(c1 != c2 for c1, c2 in zip(s1, s2))


def prefix_suffix_overlap_length(read_a: str, read_b: str, 
                                  max_errors: int = 0) -> Optional[int]:
    """
    Find the longest suffix of read_a that matches a prefix of read_b.
    
    Args:
        read_a: First sequence
        read_b: Second sequence
        max_errors: Maximum allowed mismatches
        
    Returns:
        Length of longest valid overlap, or None if no overlap found
    """
    len_a = len(read_a)
    len_b = len(read_b)
    
    # Start from maximum possible overlap and work down
    max_overlap = min(len_a, len_b)
    
    for overlap_len in range(max_overlap, 0, -1):
        suffix_a = read_a[-overlap_len:]
        prefix_b = read_b[:overlap_len]
        
        errors = hamming_distance(suffix_a, prefix_b)
        if errors <= max_errors:
            return overlap_len
    
    return None


def find_overlaps(reads: List[SequenceRecord], 
                  min_overlap: int = 20,
                  max_error_rate: float = 0.1,
                  max_errors: Optional[int] = None,
                  both_strands: bool = True,
                  max_reads: Optional[int] = None) -> List[Overlap]:
    """
    Find all suffix-prefix overlaps between reads.
    
    Args:
        reads: List of SequenceRecord objects
        min_overlap: Minimum overlap length to consider
        max_error_rate: Maximum error rate (mismatches / overlap_length)
        max_errors: Maximum absolute errors (overrides max_error_rate if set)
        both_strands: If True, also check reverse complement of read_b
        max_reads: Limit number of reads to process (for memory)
        
    Returns:
        List of Overlap objects
    """
    if max_reads is None:
        max_reads = len(reads)
    else:
        max_reads = min(max_reads, len(reads))
    
    overlaps = []
    
    for i in range(max_reads):
        seq_i = reads[i].sequence
        
        for j in range(max_reads):
            if i == j:
                continue
            
            seq_j = reads[j].sequence
            
            # Check forward strand
            overlap_len = prefix_suffix_overlap_length(seq_i, seq_j)
            if overlap_len is not None and overlap_len >= min_overlap:
                # Calculate error rate
                suffix = seq_i[-overlap_len:]
                prefix = seq_j[:overlap_len]
                errors = hamming_distance(suffix, prefix)
                
                if max_errors is not None:
                    allowed = max_errors
                else:
                    allowed = int(max_error_rate * overlap_len)
                
                if errors <= allowed:
                    score = 1.0 - (errors / overlap_len) if overlap_len > 0 else 1.0
                    overlaps.append(Overlap(
                        read_a=i,
                        read_b=j,
                        offset=len(seq_i) - overlap_len,
                        length=overlap_len,
                        score=score,
                        is_reverse=False,
                    ))
            
            # Check reverse complement
            if both_strands:
                seq_j_rc = SequenceRecord(
                    id=reads[j].id,
                    description=reads[j].description,
                    sequence=reads[j].sequence,
                ).reverse_complement().sequence
                
                overlap_len = prefix_suffix_overlap_length(seq_i, seq_j_rc)
                if overlap_len is not None and overlap_len >= min_overlap:
                    suffix = seq_i[-overlap_len:]
                    prefix = seq_j_rc[:overlap_len]
                    errors = hamming_distance(suffix, prefix)
                    
                    if max_errors is not None:
                        allowed = max_errors
                    else:
                        allowed = int(max_error_rate * overlap_len)
                    
                    if errors <= allowed:
                        score = 1.0 - (errors / overlap_len) if overlap_len > 0 else 1.0
                        overlaps.append(Overlap(
                            read_a=i,
                            read_b=j,
                            offset=len(seq_i) - overlap_len,
                            length=overlap_len,
                            score=score,
                            is_reverse=True,
                        ))
    
    return overlaps


def build_overlap_graph(reads: List[SequenceRecord],
                       overlaps: List[Overlap]) -> Dict[int, List[Overlap]]:
    """
    Build an adjacency list representation of the overlap graph.
    
    Args:
        reads: List of reads
        overlaps: List of Overlap objects
        
    Returns:
        Dictionary mapping read index to list of outgoing overlaps
    """
    graph: Dict[int, List[Overlap]] = {i: [] for i in range(len(reads))}
    
    for ov in overlaps:
        graph[ov.read_a].append(ov)
    
    return graph


def transitive_reduction(overlaps: List[Overlap]) -> List[Overlap]:
    """
    Remove transitive edges from the overlap graph.
    
    An overlap A->C is transitive if there exists B such that:
    A->B and B->C exist, and A->C is implied by them.
    
    Args:
        overlaps: List of Overlap objects
        
    Returns:
        Reduced list of overlaps
    """
    # Group overlaps by source read
    by_source: Dict[int, List[Overlap]] = {}
    for ov in overlaps:
        if ov.read_a not in by_source:
            by_source[ov.read_a] = []
        by_source[ov.read_a].append(ov)
    
    # Build a set of all overlap edges for quick lookup
    overlap_set = {(ov.read_a, ov.read_b) for ov in overlaps}
    
    # For each source, keep only direct edges
    reduced = []
    for source, ovs in by_source.items():
        # Sort by offset (closest read first)
        ovs.sort(key=lambda x: x.offset)
        
        # Keep overlaps that aren't transitive
        kept = []
        for ov in ovs:
            # Check if this overlap is transitive via another read
            is_transitive = False
            
            # Check if there's a path source -> X -> target that implies this edge
            for intermediate in range(max(ov.read_a, ov.read_b) + 1):
                if intermediate == ov.read_a or intermediate == ov.read_b:
                    continue
                if (ov.read_a, intermediate) in overlap_set and \
                   (intermediate, ov.read_b) in overlap_set:
                    # Check if the intermediate path is shorter or equal
                    # If A->B and B->C exist, then A->C might be transitive
                    # if A->C is longer than A->B + B->C
                    is_transitive = True
                    break
            
            if not is_transitive:
                kept.append(ov)
        
        reduced.extend(kept)
    
    return reduced
