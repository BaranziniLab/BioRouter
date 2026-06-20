"""
Motif simulation and testing utilities.

Generates planted motifs in random sequences for testing algorithms.
"""

import random
from typing import List, Tuple, Optional
from dataclasses import dataclass

import numpy as np


@dataclass
class PlantedMotif:
    """
    A planted motif instance.
    
    Attributes:
        motif: The planted motif sequence.
        positions: Positions where motif was implanted.
        sequences: Sequences with implanted motifs.
        mutations: Number of mutations per instance.
    """
    motif: str
    positions: List[int]
    sequences: List[str]
    mutations: int


class MotifSimulator:
    """
    Generates test sequences with planted motifs.
    """
    
    NUCLEOTIDES = ['A', 'C', 'G', 'T']
    
    def __init__(self, seed: Optional[int] = None):
        """
        Initialize simulator.
        
        Args:
            seed: Random seed for reproducibility.
        """
        self.rng = np.random.RandomState(seed)
        random.seed(seed)
    
    def generate_random_sequence(self, length: int) -> str:
        """
        Generate random DNA sequence.
        
        Args:
            length: Sequence length.
        
        Returns:
            Random DNA sequence.
        """
        return ''.join(self.rng.choice(self.NUCLEOTIDES, length))
    
    def mutate_sequence(self, sequence: str, num_mutations: int) -> str:
        """
        Introduce mutations into a sequence.
        
        Args:
            sequence: Original sequence.
            num_mutations: Number of positions to mutate.
        
        Returns:
            Mutated sequence.
        """
        seq_list = list(sequence.upper())
        positions = self.rng.choice(len(seq_list), min(num_mutations, len(seq_list)), replace=False)
        
        for pos in positions:
            original = seq_list[pos]
            # Choose a different nucleotide
            alternatives = [nuc for nuc in self.NUCLEOTIDES if nuc != original]
            seq_list[pos] = self.rng.choice(alternatives)
        
        return ''.join(seq_list)
    
    def implant_motif(self, 
                      sequences: List[str], 
                      motif: str, 
                      mutations_per_instance: int = 1,
                      min_spacing: int = 0) -> PlantedMotif:
        """
        Implant a motif into sequences with optional mutations.
        
        Args:
            sequences: Input sequences (will be modified in-place).
            motif: Motif sequence to implant.
            mutations_per_instance: Mutations to introduce in each instance.
            min_spacing: Minimum distance between implant sites.
        
        Returns:
            PlantedMotif with positions and mutated sequences.
        """
        positions = []
        mutated_sequences = []
        
        for i, seq in enumerate(sequences):
            seq_upper = seq.upper()
            seq_len = len(seq_upper)
            motif_len = len(motif)
            
            # Find valid positions
            if min_spacing > 0 and positions:
                # Ensure minimum spacing
                last_pos = positions[-1]
                start = max(0, last_pos + motif_len + min_spacing)
            else:
                start = 0
            
            # Random position
            if seq_len - motif_len >= start:
                pos = self.rng.randint(start, seq_len - motif_len + 1)
            else:
                pos = self.rng.randint(0, seq_len - motif_len + 1)
            
            # Plant motif with mutations
            mutated_motif = self.mutate_sequence(motif, mutations_per_instance)
            
            # Replace region
            new_seq = seq_upper[:pos] + mutated_motif + seq_upper[pos + motif_len:]
            mutated_sequences.append(new_seq)
            positions.append(pos)
        
        return PlantedMotif(
            motif=motif,
            positions=positions,
            sequences=mutated_sequences,
            mutations=mutations_per_instance
        )
    
    def generate_dataset(self, 
                        num_sequences: int = 20,
                        sequence_length: int = 100,
                        motif_length: int = 8,
                        motif: Optional[str] = None,
                        mutations_per_instance: int = 1,
                        background_gc: float = 0.5) -> PlantedMotif:
        """
        Generate a complete test dataset with planted motifs.
        
        Args:
            num_sequences: Number of sequences.
            sequence_length: Length of each sequence.
            motif_length: Length of motif if not specified.
            motif: Specific motif sequence (random if None).
            mutations_per_instance: Mutations per motif instance.
            background_gc: GC content of background sequences.
        
        Returns:
            PlantedMotif with all data.
        """
        # Generate random sequences
        sequences = []
        for _ in range(num_sequences):
            # Generate with specified GC content
            seq = []
            for _ in range(sequence_length):
                if self.rng.random() < background_gc:
                    # GC nucleotides
                    seq.append(self.rng.choice(['G', 'C']))
                else:
                    # AT nucleotides
                    seq.append(self.rng.choice(['A', 'T']))
            sequences.append(''.join(seq))
        
        # Generate or use provided motif
        if motif is None:
            motif = ''.join(self.rng.choice(self.NUCLEOTIDES, motif_length))
        
        # Implant motif
        return self.implant_motif(sequences, motif, mutations_per_instance)
    
    def generate_fasta(self, sequences: List[str], names: Optional[List[str]] = None) -> str:
        """
        Generate FASTA format string.
        
        Args:
            sequences: List of sequences.
            names: Optional sequence names.
        
        Returns:
            FASTA formatted string.
        """
        if names is None:
            names = [f"seq_{i}" for i in range(len(sequences))]
        
        fasta_lines = []
        for name, seq in zip(names, sequences):
            fasta_lines.append(f">{name}")
            # Wrap at 80 characters
            for i in range(0, len(seq), 80):
                fasta_lines.append(seq[i:i + 80])
        
        return '\n'.join(fasta_lines)
    
    def parse_fasta(self, fasta_string: str) -> Tuple[List[str], List[str]]:
        """
        Parse FASTA format string.
        
        Args:
            fasta_string: FASTA formatted string.
        
        Returns:
            Tuple of (sequences, names).
        """
        sequences = []
        names = []
        current_seq = []
        current_name = None
        
        for line in fasta_string.strip().split('\n'):
            line = line.strip()
            if line.startswith('>'):
                # Save previous sequence
                if current_name is not None:
                    sequences.append(''.join(current_seq))
                    names.append(current_name)
                
                current_name = line[1:].split()[0] if line[1:].strip() else f"seq_{len(sequences)}"
                current_seq = []
            elif line:
                current_seq.append(line.upper())
        
        # Save last sequence
        if current_name is not None:
            sequences.append(''.join(current_seq))
            names.append(current_name)
        
        return sequences, names


def create_test_file(filepath: str, 
                    num_sequences: int = 20,
                    sequence_length: int = 100,
                    motif_length: int = 8) -> str:
    """
    Create a FASTA test file with planted motifs.
    
    Args:
        filepath: Output file path.
        num_sequences: Number of sequences.
        sequence_length: Length of each sequence.
        motif_length: Motif length.
    
    Returns:
        The planted motif sequence.
    """
    simulator = MotifSimulator(seed=42)
    data = simulator.generate_dataset(
        num_sequences=num_sequences,
        sequence_length=sequence_length,
        motif_length=motif_length
    )
    
    fasta = simulator.generate_fasta(data.sequences)
    with open(filepath, 'w') as f:
        f.write(fasta)
    
    return data.motif
