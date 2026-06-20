"""
Scoring functions for motif analysis.

Implements information content, relative entropy, and background model scoring
for evaluating motif significance and quality.
"""

import math
from typing import Dict, List, Optional, Tuple
from collections import Counter

import numpy as np


class BackgroundModel:
    """
    DNA background model for scoring.
    
    Supports uniform and custom nucleotide frequencies.
    """
    
    NUCLEOTIDES = ['A', 'C', 'G', 'T']
    
    def __init__(self, frequencies: Optional[Dict[str, float]] = None):
        """
        Initialize background model.
        
        Args:
            frequencies: Custom nucleotide frequencies. If None, uses uniform.
        """
        if frequencies is None:
            # Uniform background
            self.frequencies = {nuc: 0.25 for nuc in self.NUCLEOTIDES}
        else:
            # Normalize custom frequencies
            total = sum(frequencies.values())
            self.frequencies = {nuc: freq / total for nuc, freq in frequencies.items()}
    
    def get_probability(self, nucleotide: str) -> float:
        """Get probability of a nucleotide."""
        return self.frequencies.get(nucleotide.upper(), 0.0)
    
    def get_log_probability(self, nucleotide: str) -> float:
        """Get log probability of a nucleotide."""
        prob = self.get_probability(nucleotide)
        if prob <= 0:
            return -float('inf')
        return math.log(prob)
    
    def score_sequence(self, sequence: str) -> float:
        """Score a sequence under the background model."""
        log_prob = 0.0
        for nuc in sequence.upper():
            log_prob += self.get_log_probability(nuc)
        return log_prob
    
    def to_dict(self) -> Dict[str, float]:
        """Convert to dictionary."""
        return self.frequencies.copy()
    
    @classmethod
    def from_sequences(cls, sequences: List[str]) -> 'BackgroundModel':
        """Create background model from sequence data."""
        counts = Counter()
        for seq in sequences:
            counts.update(seq.upper())
        
        total = sum(counts.values())
        frequencies = {nuc: counts.get(nuc, 0) / total for nuc in cls.NUCLEOTIDES}
        return cls(frequencies)


class InformationContent:
    """
    Information content scoring for motifs.
    
    Measures how much a motif differs from background, using bits.
    """
    
    def __init__(self, background: Optional[BackgroundModel] = None):
        """
        Initialize information content calculator.
        
        Args:
            background: Background model for comparison.
        """
        self.background = background or BackgroundModel()
    
    def position_ic(self, position_counts: Dict[str, int], total_sequences: int) -> float:
        """
        Calculate information content for a single position.
        
        Args:
            position_counts: Counts for each nucleotide at this position.
            total_sequences: Total number of sequences.
        
        Returns:
            Information content in bits (0 to 2).
        """
        ic = 0.0
        for nuc in ['A', 'C', 'G', 'T']:
            count = position_counts.get(nuc, 0)
            if count > 0:
                # Observed frequency
                freq = count / total_sequences
                
                # Expected frequency under background
                bg_freq = self.background.get_probability(nuc)
                
                # Information content: D(P||Q) = sum(P*log(P/Q))
                ic += freq * math.log2(freq / bg_freq)
        
        return ic
    
    def motif_ic(self, counts_matrix: List[Dict[str, int]], total_sequences: int) -> float:
        """
        Calculate total information content for a motif.
        
        Args:
            counts_matrix: List of position counts.
            total_sequences: Total number of sequences.
        
        Returns:
            Total information content in bits.
        """
        total_ic = 0.0
        for position_counts in counts_matrix:
            total_ic += self.position_ic(position_counts, total_sequences)
        return total_ic
    
    def relative_entropy(self, observed: float, expected: float) -> float:
        """
        Calculate relative entropy (KL divergence) at a position.
        
        Args:
            observed: Observed probability.
            expected: Expected probability under background.
        
        Returns:
            KL divergence value.
        """
        if observed <= 0 or expected <= 0:
            return 0.0
        return observed * math.log2(observed / expected)


class MotifScorer:
    """
    Comprehensive scoring for motifs using PWM and information content.
    """
    
    def __init__(self, background: Optional[BackgroundModel] = None):
        """
        Initialize motif scorer.
        
        Args:
            background: Background model for scoring.
        """
        self.background = background or BackgroundModel()
        self.ic_calculator = InformationContent(self.background)
    
    def calculate_log_odds(self, pwm: 'PWM') -> np.ndarray:
        """
        Calculate log-odds scores for a PWM.
        
        Args:
            pwm: Position Weight Matrix.
        
        Returns:
            Log-odds score matrix.
        """
        nuc_to_idx = {'A': 0, 'C': 1, 'G': 2, 'T': 3}
        log_odds = np.zeros((4, pwm.length))
        
        for i, nuc in enumerate(['A', 'C', 'G', 'T']):
            bg_prob = self.background.get_probability(nuc)
            if bg_prob > 0:
                for j in range(pwm.length):
                    pwm_prob = pwm.get_probability(nuc, j)
                    if pwm_prob > 0:
                        log_odds[i, j] = math.log2(pwm_prob / bg_prob)
                    else:
                        log_odds[i, j] = -float('inf')
            else:
                log_odds[i, :] = -float('inf')
        
        return log_odds
    
    def score_site(self, pwm: 'PWM', sequence: str) -> float:
        """
        Score a sequence site against a PWM.
        
        Args:
            pwm: Position Weight Matrix.
            sequence: Sequence to score.
        
        Returns:
            Log-odds score.
        """
        log_odds = self.calculate_log_odds(pwm)
        score = 0.0
        nuc_to_idx = {'A': 0, 'C': 1, 'G': 2, 'T': 3}
        
        for j, nuc in enumerate(sequence.upper()):
            if nuc in nuc_to_idx:
                score += log_odds[nuc_to_idx[nuc], j]
            else:
                score = -float('inf')
                break
        
        return score
    
    def scan_sequence(self, pwm: 'PWM', sequence: str, threshold: float = 0.0) -> List[Tuple[int, float]]:
        """
        Scan a sequence for motif matches above threshold.
        
        Args:
            pwm: Position Weight Matrix.
            sequence: Sequence to scan.
            threshold: Minimum score threshold.
        
        Returns:
            List of (position, score) tuples.
        """
        matches = []
        seq_upper = sequence.upper()
        
        for i in range(len(seq_upper) - pwm.length + 1):
            site = seq_upper[i:i + pwm.length]
            score = self.score_site(pwm, site)
            
            if score >= threshold:
                matches.append((i, score))
        
        return matches
    
    def consensus_score(self, pwm: 'PWM', consensus: str) -> float:
        """
        Score a consensus sequence against the PWM.
        
        Args:
            pwm: Position Weight Matrix.
            consensus: Consensus sequence.
        
        Returns:
            Score of the consensus.
        """
        return self.score_site(pwm, consensus)
