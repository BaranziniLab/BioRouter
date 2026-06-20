"""
Gibbs sampling algorithm for motif discovery.

Implements the Gibbs sampling approach for finding motifs in DNA sequences.
"""

import random
import math
from typing import List, Optional, Tuple, Dict
from collections import Counter

import numpy as np

from bio_motif_finder.pwm import PWM
from bio_motif_finder.score import BackgroundModel, MotifScorer


class GibbsSampler:
    """
    Gibbs sampling algorithm for motif discovery.
    
    Iteratively samples motif occurrences from sequences while
    updating the position weight matrix.
    """
    
    NUCLEOTIDES = ['A', 'C', 'G', 'T']
    
    def __init__(self,
                 motif_width: int = 8,
                 num_iterations: int = 1000,
                 background: Optional[BackgroundModel] = None,
                 pseudocount: float = 1.0):
        """
        Initialize Gibbs sampler.
        
        Args:
            motif_width: Width of motifs to find.
            num_iterations: Number of sampling iterations.
            background: Background model for scoring.
            pseudocount: Pseudocount for PWM construction.
        """
        self.motif_width = motif_width
        self.num_iterations = num_iterations
        self.background = background or BackgroundModel()
        self.pseudocount = pseudocount
        self.scorer = MotifScorer(self.background)
    
    def _initialize_positions(self, sequences: List[str]) -> List[int]:
        """
        Randomly initialize motif positions.
        
        Args:
            sequences: List of sequences.
        
        Returns:
            Initial positions.
        """
        positions = []
        for seq in sequences:
            max_pos = len(seq) - self.motif_width
            positions.append(random.randint(0, max_pos))
        return positions
    
    def _build_pwm(self, 
                   sequences: List[str], 
                   positions: List[int], 
                   exclude_index: int) -> PWM:
        """
        Build PWM from current positions, excluding one sequence.
        
        Args:
            sequences: List of sequences.
            positions: Current motif positions.
            exclude_index: Index of sequence to exclude.
        
        Returns:
            PWM built from remaining sequences.
        """
        site_sequences = []
        
        for i, (seq, pos) in enumerate(zip(sequences, positions)):
            if i != exclude_index:
                site = seq[pos:pos + self.motif_width]
                site_sequences.append(site.upper())
        
        return PWM.from_sequences(site_sequences, self.pseudocount)
    
    def _sample_position(self, 
                        sequence: str, 
                        pwm: PWM) -> int:
        """
        Sample a position from the probability distribution.
        
        Args:
            sequence: Sequence to sample from.
            pwm: Current PWM.
        
        Returns:
            Sampled position.
        """
        seq_upper = sequence.upper()
        scores = []
        
        # Calculate scores for all positions
        for i in range(len(seq_upper) - self.motif_width + 1):
            site = seq_upper[i:i + self.motif_width]
            score = self.scorer.score_site(pwm, site)
            scores.append(score)
        
        # Convert to probabilities using softmax
        max_score = max(scores)
        exp_scores = [math.exp(s - max_score) for s in scores]
        total = sum(exp_scores)
        probabilities = [s / total for s in exp_scores]
        
        # Sample from distribution
        r = random.random()
        cumulative = 0.0
        
        for i, prob in enumerate(probabilities):
            cumulative += prob
            if r <= cumulative:
                return i
        
        return len(probabilities) - 1
    
    def _calculate_conservation(self, pwm: PWM) -> float:
        """
        Calculate PWM conservation (information content).
        
        Args:
            pwm: Position Weight Matrix.
        
        Returns:
            Conservation score.
        """
        total_ic = 0.0
        
        for j in range(pwm.length):
            for nuc in self.NUCLEOTIDES:
                prob = pwm.get_probability(nuc, j)
                bg_prob = self.background.get_probability(nuc)
                if prob > 0 and bg_prob > 0:
                    total_ic += prob * math.log2(prob / bg_prob)
        
        return total_ic / pwm.length
    
    def run(self, sequences: List[str], seed: Optional[int] = None) -> Dict:
        """
        Run Gibbs sampling.
        
        Args:
            sequences: List of sequences.
            seed: Random seed for reproducibility.
        
        Returns:
            Dictionary with results.
        """
        if seed is not None:
            random.seed(seed)
            np.random.seed(seed)
        
        n_sequences = len(sequences)
        
        # Initialize positions
        positions = self._initialize_positions(sequences)
        
        best_pwm = None
        best_conservation = -float('inf')
        best_positions = positions.copy()
        
        # Gibbs sampling iterations
        for iteration in range(self.num_iterations):
            # Choose a random sequence to exclude
            exclude_idx = random.randint(0, n_sequences - 1)
            
            # Build PWM from other sequences
            pwm = self._build_pwm(sequences, positions, exclude_idx)
            
            # Sample new position for excluded sequence
            new_pos = self._sample_position(sequences[exclude_idx], pwm)
            positions[exclude_idx] = new_pos
            
            # Track best solution
            if iteration % 10 == 0:
                # Build full PWM for evaluation
                full_pwm = self._build_pwm_full(sequences, positions)
                conservation = self._calculate_conservation(full_pwm)
                
                if conservation > best_conservation:
                    best_conservation = conservation
                    best_pwm = full_pwm
                    best_positions = positions.copy()
        
        # Extract results
        sites = []
        site_sequences = []
        
        for i, (seq, pos) in enumerate(zip(sequences, positions)):
            site = seq[pos:pos + self.motif_width]
            site_sequences.append(site.upper())
            sites.append({
                'sequence_index': i,
                'position': pos,
                'site': site.upper()
            })
        
        # Build final PWM
        final_pwm = PWM.from_sequences(site_sequences, self.pseudocount)
        consensus = final_pwm.consensus()
        
        return {
            'motif': consensus,
            'consensus': consensus,
            'sites': sites,
            'pwm': final_pwm,
            'conservation': best_conservation,
            'iterations': self.num_iterations,
            'method': 'gibbs'
        }
    
    def _build_pwm_full(self, 
                       sequences: List[str], 
                       positions: List[int]) -> PWM:
        """Build PWM from all sequences."""
        site_sequences = []
        
        for seq, pos in zip(sequences, positions):
            site = seq[pos:pos + self.motif_width]
            site_sequences.append(site.upper())
        
        return PWM.from_sequences(site_sequences, self.pseudocount)
    
    def find_motif(self, 
                  sequences: List[str],
                  num_starts: int = 10,
                  seed: Optional[int] = None) -> Dict:
        """
        Find best motif using multiple random starts.
        
        Args:
            sequences: List of sequences.
            num_starts: Number of random restarts.
            seed: Initial random seed.
        
        Returns:
            Best motif found.
        """
        best_result = None
        best_conservation = -float('inf')
        
        for i in range(num_starts):
            current_seed = (seed + i) if seed is not None else None
            result = self.run(sequences, seed=current_seed)
            
            if result['conservation'] > best_conservation:
                best_conservation = result['conservation']
                best_result = result
        
        return best_result
