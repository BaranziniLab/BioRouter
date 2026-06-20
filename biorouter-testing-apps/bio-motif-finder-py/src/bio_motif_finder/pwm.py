"""
Position Weight Matrix (PWM) implementation.

Provides PWM construction, manipulation, and utilities for motif analysis.
"""

import math
from typing import Dict, List, Optional, Tuple
from collections import Counter

import numpy as np

from bio_motif_finder.score import BackgroundModel, InformationContent


class PWM:
    """
    Position Weight Matrix for DNA motifs.
    
    Stores probabilities for each nucleotide at each position.
    """
    
    NUCLEOTIDES = ['A', 'C', 'G', 'T']
    
    def __init__(self, counts_matrix: Optional[List[Dict[str, int]]] = None,
                 pseudocount: float = 1.0):
        """
        Initialize PWM.
        
        Args:
            counts_matrix: List of position count dictionaries.
            pseudocount: Laplace pseudocount for smoothing.
        """
        self.pseudocount = pseudocount
        self.length = 0
        self.counts = []
        self.probabilities = []
        
        if counts_matrix is not None:
            self.length = len(counts_matrix)
            self.counts = counts_matrix
            self._calculate_probabilities()
    
    def _calculate_probabilities(self) -> None:
        """Calculate probabilities from counts with pseudocounts."""
        self.probabilities = []
        for position_counts in self.counts:
            total = sum(position_counts.values()) + 4 * self.pseudocount
            probs = {}
            for nuc in self.NUCLEOTIDES:
                count = position_counts.get(nuc, 0) + self.pseudocount
                probs[nuc] = count / total
            self.probabilities.append(probs)
    
    @classmethod
    def from_sequences(cls, sequences: List[str], pseudocount: float = 1.0) -> 'PWM':
        """
        Create PWM from aligned sequences.
        
        Args:
            sequences: List of aligned sequences (same length).
            pseudocount: Laplace pseudocount.
        
        Returns:
            PWM instance.
        """
        if not sequences:
            raise ValueError("No sequences provided")
        
        length = len(sequences[0])
        for seq in sequences:
            if len(seq) != length:
                raise ValueError("Sequences must be aligned (same length)")
        
        # Count nucleotides at each position
        counts_matrix = []
        for j in range(length):
            position_counts = Counter()
            for seq in sequences:
                nuc = seq[j].upper()
                if nuc in cls.NUCLEOTIDES:
                    position_counts[nuc] += 1
            counts_matrix.append(dict(position_counts))
        
        return cls(counts_matrix, pseudocount)
    
    @classmethod
    def from_counts(cls, counts: List[Dict[str, int]], pseudocount: float = 1.0) -> 'PWM':
        """
        Create PWM from explicit counts.
        
        Args:
            counts: List of position count dictionaries.
            pseudocount: Laplace pseudocount.
        
        Returns:
            PWM instance.
        """
        return cls(counts, pseudocount)
    
    @classmethod
    def random(cls, length: int, pseudocount: float = 1.0) -> 'PWM':
        """
        Create random PWM.
        
        Args:
            length: PWM length.
            pseudocount: Pseudocount.
        
        Returns:
            Random PWM.
        """
        counts_matrix = []
        for _ in range(length):
            # Random counts (1-10 for each nucleotide)
            counts = {nuc: np.random.randint(1, 11) for nuc in cls.NUCLEOTIDES}
            counts_matrix.append(counts)
        return cls(counts_matrix, pseudocount)
    
    def get_probability(self, nucleotide: str, position: int) -> float:
        """
        Get probability of nucleotide at position.
        
        Args:
            nucleotide: DNA base (A, C, G, T).
            position: Position index.
        
        Returns:
            Probability value.
        """
        if position < 0 or position >= self.length:
            raise IndexError(f"Position {position} out of range")
        return self.probabilities[position].get(nucleotide.upper(), 0.0)
    
    def get_counts(self, position: int) -> Dict[str, int]:
        """Get counts at a position."""
        if position < 0 or position >= self.length:
            raise IndexError(f"Position {position} out of range")
        return self.counts[position].copy()
    
    def consensus(self) -> str:
        """
        Extract consensus sequence.
        
        Returns:
            Consensus sequence (most frequent nucleotide at each position).
        """
        consensus_seq = []
        for position_probs in self.probabilities:
            max_nuc = max(position_probs, key=position_probs.get)
            consensus_seq.append(max_nuc)
        return ''.join(consensus_seq)
    
    def weblogo_data(self) -> Dict[int, Dict[str, float]]:
        """
        Get data for sequence logo visualization.
        
        Returns:
            Dictionary mapping positions to nucleotide heights.
        """
        logo_data = {}
        for j in range(self.length):
            # Calculate information content
            ic = 0.0
            probs = self.probabilities[j]
            for prob in probs.values():
                if prob > 0:
                    ic -= prob * math.log2(prob)
            
            # Scale heights by information content
            logo_data[j] = {nuc: probs[nuc] * ic for nuc in self.NUCLEOTIDES}
        
        return logo_data
    
    def to_dict(self) -> List[Dict[str, float]]:
        """Convert to list of probability dictionaries."""
        return self.probabilities.copy()
    
    def __len__(self) -> int:
        """Return PWM length."""
        return self.length
    
    def __repr__(self) -> str:
        """String representation."""
        return f"PWM(length={self.length}, pseudocount={self.pseudocount})"
    
    def similarity(self, other: 'PWM') -> float:
        """
        Calculate similarity between two PWMs.
        
        Args:
            other: Another PWM to compare.
        
        Returns:
            Similarity score (0 to 1).
        """
        if self.length != other.length:
            raise ValueError("PWMs must have same length")
        
        total_similarity = 0.0
        for j in range(self.length):
            for nuc in self.NUCLEOTIDES:
                p1 = self.get_probability(nuc, j)
                p2 = other.get_probability(nuc, j)
                # Bhattacharyya coefficient
                total_similarity += math.sqrt(p1 * p2)
        
        return total_similarity / self.length
    
    def reverse_complement(self) -> 'PWM':
        """
        Create reverse complement PWM.
        
        Returns:
            Reverse complement PWM.
        """
        complement = {'A': 'T', 'T': 'A', 'C': 'G', 'G': 'C'}
        
        new_counts = []
        for j in reversed(range(self.length)):
            old_counts = self.counts[j]
            new_counts.append({complement[nuc]: count for nuc, count in old_counts.items()})
        
        return PWM(new_counts, self.pseudocount)
    
    def trim(self, start: int, end: int) -> 'PWM':
        """
        Trim PWM to a sub-region.
        
        Args:
            start: Start position (inclusive).
            end: End position (exclusive).
        
        Returns:
            Trimmed PWM.
        """
        if start < 0 or end > self.length or start >= end:
            raise ValueError("Invalid trim positions")
        
        return PWM(self.counts[start:end], self.pseudocount)


class PWMSet:
    """
    Collection of PWMs for motif analysis.
    """
    
    def __init__(self):
        """Initialize empty PWM set."""
        self.pwms: List[PWM] = []
        self.names: List[str] = []
    
    def add(self, pwm: PWM, name: str = "") -> None:
        """Add a PWM with optional name."""
        self.pwms.append(pwm)
        self.names.append(name)
    
    def get_best(self, scorer: 'MotifScorer') -> PWM:
        """
        Get PWM with highest average information content.
        
        Args:
            scorer: Scorer for evaluation.
        
        Returns:
            Best PWM.
        """
        if not self.pwms:
            raise ValueError("No PWMs in set")
        
        best_pwm = None
        best_score = -float('inf')
        
        for pwm in self.pwms:
            # Calculate average IC
            total_ic = 0.0
            for j in range(pwm.length):
                counts = pwm.get_counts(j)
                total_ic += scorer.ic_calculator.position_ic(counts, sum(counts.values()))
            
            if total_ic > best_score:
                best_score = total_ic
                best_pwm = pwm
        
        return best_pwm
