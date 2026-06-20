"""
Read simulator for testing genome assemblers.

Generates simulated reads from a reference sequence by:
- Fragmenting the reference into overlapping reads
- Optionally introducing errors (substitutions, insertions, deletions)
- Supporting both long reads (ONT/PacBio-like) and short reads (Illumina-like)
"""

from __future__ import annotations

import random
from typing import List, Optional, Tuple

from .io import SequenceRecord


def generate_random_sequence(length: int, 
                            gc_content: float = 0.5,
                            seed: Optional[int] = None) -> str:
    """
    Generate a random DNA sequence with specified GC content.
    
    Args:
        length: Length of sequence to generate
        gc_content: Target GC content (0.0 - 1.0)
        seed: Random seed for reproducibility
        
    Returns:
        Random DNA sequence string
    """
    if seed is not None:
        random.seed(seed)
    
    # Calculate base probabilities
    at_prob = (1.0 - gc_content) / 2
    gc_prob = gc_content / 2
    
    bases = ["A", "T", "G", "C"]
    probs = [at_prob, at_prob, gc_prob, gc_prob]
    
    return "".join(random.choices(bases, weights=probs, k=length))


def simulate_long_reads(reference: str,
                       num_reads: int = 100,
                       read_length: int = 10000,
                       error_rate: float = 0.01,
                       seed: Optional[int] = None,
                       prefix: str = "read") -> List[SequenceRecord]:
    """
    Simulate long reads (like Nanopore/PacBio) from a reference.
    
    Reads are sampled from random positions with some overlap.
    
    Args:
        reference: Reference sequence
        num_reads: Number of reads to simulate
        read_length: Average read length (will vary)
        error_rate: Error rate per base (substitutions only for simplicity)
        seed: Random seed
        prefix: Read ID prefix
        
    Returns:
        List of SequenceRecord objects
    """
    if seed is not None:
        random.seed(seed)
    
    reads = []
    ref_len = len(reference)
    
    for i in range(num_reads):
        # Random position (allow some reads to extend past end)
        pos = random.randint(0, max(0, ref_len - 1))
        
        # Random length around mean
        length = max(100, int(random.gauss(read_length, read_length * 0.2)))
        length = min(length, ref_len - pos)
        
        if length <= 0:
            continue
        
        # Extract sequence
        seq = reference[pos:pos + length]
        
        # Introduce errors
        if error_rate > 0:
            seq = _introduce_errors(seq, error_rate, "substitution")
        
        reads.append(SequenceRecord(
            id=f"{prefix}_{i + 1:06d}",
            description=f"simulated long read from position {pos}",
            sequence=seq,
        ))
    
    return reads


def simulate_short_reads(reference: str,
                        num_reads: int = 1000,
                        read_length: int = 150,
                        insert_size: int = 300,
                        error_rate: float = 0.001,
                        seed: Optional[int] = None,
                        prefix: str = "read") -> List[SequenceRecord]:
    """
    Simulate short paired-end reads (like Illumina) from a reference.
    
    Args:
        reference: Reference sequence
        num_reads: Number of read pairs (will produce 2x reads)
        read_length: Length of each read in pair
        insert_size: Distance between read pairs
        error_rate: Error rate per base
        seed: Random seed
        prefix: Read ID prefix
        
    Returns:
        List of SequenceRecord objects (R1 and R2 interleaved)
    """
    if seed is not None:
        random.seed(seed)
    
    reads = []
    ref_len = len(reference)
    
    for i in range(num_reads):
        # Random position for the pair
        pos = random.randint(0, max(0, ref_len - insert_size - read_length))
        
        # R1 from start of fragment
        r1_start = pos
        r1_seq = reference[r1_start:r1_start + read_length]
        
        # R2 from end of fragment (reverse complement implied)
        r2_start = pos + insert_size - read_length
        r2_seq = reference[r2_start:r2_start + read_length]
        r2_seq = _reverse_complement(r2_seq)
        
        # Introduce errors
        if error_rate > 0:
            r1_seq = _introduce_errors(r1_seq, error_rate, "substitution")
            r2_seq = _introduce_errors(r2_seq, error_rate, "substitution")
        
        reads.append(SequenceRecord(
            id=f"{prefix}_{i + 1:06d}:1",
            description=f"simulated R1 from position {pos}",
            sequence=r1_seq,
            quality="I" * len(r1_seq),
        ))
        
        reads.append(SequenceRecord(
            id=f"{prefix}_{i + 1:06d}:2",
            description=f"simulated R2 from position {pos}",
            sequence=r2_seq,
            quality="I" * len(r2_seq),
        ))
    
    return reads


def simulate_reads_from_file(reference_file: str,
                            output_file: str,
                            num_reads: int = 1000,
                            read_length: int = 150,
                            error_rate: float = 0.001,
                            seed: Optional[int] = None) -> None:
    """
    Simulate reads from a reference file and write to FASTQ.
    
    Args:
        reference_file: Path to reference FASTA file
        output_file: Output FASTQ file path
        num_reads: Number of reads to simulate
        read_length: Read length
        error_rate: Error rate per base
        seed: Random seed
    """
    from .io import read_fasta, write_fastq
    
    # Read reference
    records = list(read_fasta(reference_file))
    if not records:
        raise ValueError("Reference file is empty")
    
    reference = records[0].sequence
    
    # Simulate reads
    reads = simulate_short_reads(
        reference,
        num_reads=num_reads // 2,
        read_length=read_length,
        error_rate=error_rate,
        seed=seed,
    )
    
    # Write output
    write_fastq(reads, output_file)


def _introduce_errors(sequence: str, error_rate: float, 
                     error_type: str = "substitution") -> str:
    """
    Introduce random errors into a sequence.
    
    Args:
        sequence: Input sequence
        error_rate: Probability of error per base
        error_type: Type of error ("substitution", "insertion", "deletion")
        
    Returns:
        Sequence with errors introduced
    """
    bases = ["A", "T", "G", "C"]
    result = []
    
    for base in sequence.upper():
        if random.random() < error_rate:
            if error_type == "substitution":
                # Replace with different base
                alternatives = [b for b in bases if b != base]
                result.append(random.choice(alternatives))
            elif error_type == "insertion":
                result.append(base)
                result.append(random.choice(bases))
            elif error_type == "deletion":
                continue  # Skip this base
            else:
                result.append(base)
        else:
            result.append(base)
    
    return "".join(result)


def _reverse_complement(sequence: str) -> str:
    """Return the reverse complement of a DNA sequence."""
    comp = str.maketrans("ACGTacgt", "TGCAtgca")
    return sequence[::-1].translate(comp)


def create_test_reference(length: int = 10000, 
                         seed: int = 42,
                         pattern: str = "random") -> str:
    """
    Create a test reference sequence for assembly testing.
    
    Args:
        length: Length of reference
        seed: Random seed
        pattern: Type of pattern ("random", "repeat", "simple")
        
    Returns:
        Reference sequence string
    """
    if pattern == "simple":
        # Simple repeating pattern
        unit = "ACGTACGT"
        return (unit * (length // len(unit) + 1))[:length]
    elif pattern == "repeat":
        # Contains some repeats
        random.seed(seed)
        segments = []
        remaining = length
        while remaining > 0:
            seg_len = min(remaining, random.randint(100, 500))
            seg = generate_random_sequence(seg_len, seed=None)
            segments.append(seg)
            remaining -= seg_len
        return "".join(segments)
    else:  # random
        return generate_random_sequence(length, seed=seed)
