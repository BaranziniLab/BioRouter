"""
Consensus sequence generation from aligned overlaps.

Provides simple majority-rule consensus for overlapping read regions,
and can merge reads into contigs based on overlap information.
"""

from __future__ import annotations

from collections import Counter
from typing import Dict, List, Optional, Tuple

from .io import SequenceRecord


def simple_consensus(sequences: List[str], weights: Optional[List[float]] = None) -> str:
    """
    Generate a consensus sequence from multiple aligned sequences using majority rule.
    
    Args:
        sequences: List of aligned sequences (all same length)
        weights: Optional weights for each sequence
        
    Returns:
        Consensus sequence string
    """
    if not sequences:
        raise ValueError("No sequences provided")
    
    length = len(sequences[0])
    if not all(len(s) == length for s in sequences):
        raise ValueError("All sequences must have the same length")
    
    consensus = []
    for pos in range(length):
        counter: Counter[str] = Counter()
        for i, seq in enumerate(sequences):
            base = seq[pos].upper()
            if base in "ACGTN":
                weight = weights[i] if weights else 1.0
                counter[base] += weight
        
        # Get the base with highest count
        if counter:
            best = counter.most_common(1)[0][0]
            consensus.append(best)
        else:
            consensus.append("N")
    
    return "".join(consensus)


def merge_two_reads(read_a: str, read_b: str, overlap_length: int) -> str:
    """
    Merge two reads based on their suffix-prefix overlap.
    
    Args:
        read_a: First read sequence
        read_b: Second read sequence  
        overlap_length: Length of overlap between them
        
    Returns:
        Merged sequence
    """
    if overlap_length <= 0:
        # No overlap, just concatenate with Ns in between
        return read_a + "N" * 10 + read_b
    
    if overlap_length > len(read_a) or overlap_length > len(read_b):
        raise ValueError("Overlap length exceeds read lengths")
    
    # Take full read_a, then append non-overlapping part of read_b
    return read_a + read_b[overlap_length:]


def consensus_from_paths(reads: List[SequenceRecord],
                        paths: List[List[int]],
                        overlaps: Dict[int, List]) -> List[SequenceRecord]:
    """
    Generate consensus sequences from assembly paths through reads.
    
    Args:
        reads: Original read sequences
        paths: List of paths (each path is a list of read indices)
        overlaps: Overlap information
        
    Returns:
        List of contig SequenceRecord objects
    """
    contigs = []
    
    for path_idx, path in enumerate(paths):
        if not path:
            continue
        
        # Start with first read
        current_seq = reads[path[0]].sequence
        current_qual = [1.0] * len(current_seq)
        
        # Merge subsequent reads in the path
        for i in range(1, len(path)):
            read_idx = path[i]
            next_seq = reads[read_idx].sequence
            
            # Find overlap between current and next read
            overlap_len = _find_overlap_length(current_seq, next_seq)
            
            if overlap_len > 0:
                # Generate consensus in overlap region
                overlap_a = current_seq[-overlap_len:]
                overlap_b = next_seq[:overlap_len]
                consensus_overlap = _weighted_consensus_pair(overlap_a, overlap_b)
                
                # Reconstruct: everything before overlap + consensus + everything after
                current_seq = current_seq[:-overlap_len] + consensus_overlap + next_seq[overlap_len:]
            else:
                # No significant overlap, just concatenate
                current_seq = current_seq + "N" * 5 + next_seq
        
        contigs.append(SequenceRecord(
            id=f"contig_{path_idx + 1}",
            description=f"assembled from {len(path)} reads",
            sequence=current_seq,
        ))
    
    return contigs


def _find_overlap_length(seq_a: str, seq_b: str, min_overlap: int = 10) -> int:
    """Find the length of suffix-prefix overlap between two sequences."""
    max_possible = min(len(seq_a), len(seq_b))
    
    for ov_len in range(max_possible, min_overlap - 1, -1):
        suffix = seq_a[-ov_len:]
        prefix = seq_b[:ov_len]
        
        # Quick check: count mismatches
        mismatches = sum(1 for a, b in zip(suffix, prefix) if a != b)
        error_rate = mismatches / ov_len if ov_len > 0 else 0
        
        if error_rate <= 0.1:  # Allow 10% error
            return ov_len
    
    return 0


def _weighted_consensus_pair(seq_a: str, seq_b: str, 
                            weight_a: float = 1.0, 
                            weight_b: float = 1.0) -> str:
    """Generate consensus from two sequences with weights."""
    result = []
    for a, b in zip(seq_a, seq_b):
        if a == b:
            result.append(a)
        elif weight_a > weight_b:
            result.append(a)
        elif weight_b > weight_a:
            result.append(b)
        else:
            # Equal weights, use base that's not N
            if a != "N":
                result.append(a)
            elif b != "N":
                result.append(b)
            else:
                result.append("N")
    
    return "".join(result)


def polish_consensus(consensus: str, reads: List[str], 
                     positions: List[int]) -> str:
    """
    Polish a consensus sequence using mapped reads.
    
    Args:
        consensus: Initial consensus sequence
        reads: List of read sequences mapped to this contig
        positions: Start position of each read in the consensus
        
    Returns:
        Polished consensus sequence
    """
    if not reads:
        return consensus
    
    seq_len = len(consensus)
    result = list(consensus)
    
    for pos, read in zip(positions, reads):
        for i, base in enumerate(read):
            target_pos = pos + i
            if 0 <= target_pos < seq_len:
                # Simple majority: if consensus is N, use read base
                if result[target_pos] == "N":
                    result[target_pos] = base.upper()
    
    return "".join(result)
