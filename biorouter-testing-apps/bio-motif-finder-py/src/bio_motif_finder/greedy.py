"""
Greedy median-string motif finding algorithm.

Implements brute-force and greedy approaches for small motif widths.
"""

import itertools
from typing import List, Optional, Tuple, Dict
from collections import Counter

from bio_motif_finder.pwm import PWM
from bio_motif_finder.score import BackgroundModel, MotifScorer


class GreedyMotifFinder:
    """
    Greedy median-string algorithm for motif discovery.
    
    For small motif widths, exhaustively searches all possible motifs
    and finds the one minimizing total Hamming distance to the best
    substring in each sequence.
    """
    
    NUCLEOTIDES = ['A', 'C', 'G', 'T']
    
    def __init__(self, 
                 motif_width: int = 8,
                 max_width_brute: int = 8,
                 background: Optional[BackgroundModel] = None):
        """
        Initialize greedy motif finder.
        
        Args:
            motif_width: Width of motifs to find.
            max_width_brute: Maximum width for brute-force.
            background: Background model for scoring.
        """
        self.motif_width = motif_width
        self.max_width_brute = max_width_brute
        self.background = background or BackgroundModel()
        self.scorer = MotifScorer(self.background)
    
    def hamming_distance(self, seq1: str, seq2: str) -> int:
        """Calculate Hamming distance between two strings."""
        return sum(c1 != c2 for c1, c2 in zip(seq1.upper(), seq2.upper()))
    
    def median_string_distance(self, 
                               candidate: str, 
                               sequences: List[str]) -> int:
        """
        Calculate total distance from candidate to best match in each sequence.
        
        Args:
            candidate: Candidate motif string.
            sequences: List of sequences.
        
        Returns:
            Total Hamming distance.
        """
        total_distance = 0
        
        for seq in sequences:
            # Find best match in this sequence
            best_distance = float('inf')
            seq_upper = seq.upper()
            
            for i in range(len(seq_upper) - len(candidate) + 1):
                substring = seq_upper[i:i + len(candidate)]
                distance = self.hamming_distance(candidate, substring)
                best_distance = min(best_distance, distance)
            
            total_distance += best_distance
        
        return total_distance
    
    def find_best_substring(self, 
                           candidate: str, 
                           sequence: str) -> Tuple[str, int, int]:
        """
        Find the best matching substring for a candidate in a sequence.
        
        Args:
            candidate: Candidate motif.
            sequence: Sequence to search.
        
        Returns:
            Tuple of (best_substring, position, hamming_distance).
        """
        best_distance = float('inf')
        best_substring = None
        best_position = 0
        
        seq_upper = sequence.upper()
        candidate_upper = candidate.upper()
        
        for i in range(len(seq_upper) - len(candidate_upper) + 1):
            substring = seq_upper[i:i + len(candidate_upper)]
            distance = self.hamming_distance(candidate_upper, substring)
            
            if distance < best_distance:
                best_distance = distance
                best_substring = substring
                best_position = i
        
        return best_substring, best_position, best_distance
    
    def brute_force_search(self, sequences: List[str]) -> Tuple[str, int, List[Tuple[str, int]]]:
        """
        Exhaustively search all possible motifs.
        
        Args:
            sequences: List of sequences.
        
        Returns:
            Tuple of (best_motif, total_distance, matches).
        """
        if self.motif_width > self.max_width_brute:
            raise ValueError(f"Width {self.motif_width} too large for brute-force (max {self.max_width_brute})")
        
        best_motif = None
        best_distance = float('inf')
        best_matches = []
        
        # Generate all possible motifs
        for motif_tuple in itertools.product(self.NUCLEOTIDES, repeat=self.motif_width):
            motif = ''.join(motif_tuple)
            
            # Calculate total distance
            total_distance = 0
            matches = []
            
            for seq in sequences:
                substring, position, distance = self.find_best_substring(motif, seq)
                total_distance += distance
                matches.append((substring, position))
            
            if total_distance < best_distance:
                best_distance = total_distance
                best_motif = motif
                best_matches = matches
        
        return best_motif, best_distance, best_matches
    
    def greedy_search(self, 
                     sequences: List[str],
                     num_iterations: int = 100) -> Tuple[str, int, List[Tuple[str, int]]]:
        """
        Greedy search with random initialization.
        
        Args:
            sequences: List of sequences.
            num_iterations: Number of random starts.
        
        Returns:
            Tuple of (best_motif, total_distance, matches).
        """
        import random
        
        best_motif = None
        best_distance = float('inf')
        best_matches = []
        
        for _ in range(num_iterations):
            # Random starting motif
            initial_motif = ''.join(random.choice(self.NUCLEOTIDES) for _ in range(self.motif_width))
            
            # Greedy hill climbing
            current_motif = initial_motif
            current_distance = self.median_string_distance(current_motif, sequences)
            
            improved = True
            while improved:
                improved = False
                
                # Try all single-nucleotide changes
                for i in range(len(current_motif)):
                    for nuc in self.NUCLEOTIDES:
                        if nuc != current_motif[i]:
                            new_motif = current_motif[:i] + nuc + current_motif[i+1:]
                            new_distance = self.median_string_distance(new_motif, sequences)
                            
                            if new_distance < current_distance:
                                current_motif = new_motif
                                current_distance = new_distance
                                improved = True
                                break
                    if improved:
                        break
            
            if current_distance < best_distance:
                best_distance = current_distance
                best_motif = current_motif
                
                # Record matches
                best_matches = []
                for seq in sequences:
                    substring, position, distance = self.find_best_substring(current_motif, seq)
                    best_matches.append((substring, position))
        
        return best_motif, best_distance, best_matches
    
    def find_motif(self, 
                  sequences: List[str],
                  method: str = 'auto') -> Dict:
        """
        Find motif using specified method.
        
        Args:
            sequences: List of sequences.
            method: 'brute', 'greedy', or 'auto'.
        
        Returns:
            Dictionary with results.
        """
        if method == 'auto':
            method = 'brute' if self.motif_width <= self.max_width_brute else 'greedy'
        
        if method == 'brute':
            motif, distance, matches = self.brute_force_search(sequences)
        elif method == 'greedy':
            motif, distance, matches = self.greedy_search(sequences)
        else:
            raise ValueError(f"Unknown method: {method}")
        
        # Extract aligned sites
        sites = []
        for i, (seq, (substring, position)) in enumerate(zip(sequences, matches)):
            sites.append({
                'sequence_index': i,
                'position': position,
                'site': substring,
                'hamming_distance': self.hamming_distance(motif, substring)
            })
        
        # Build PWM from sites
        site_sequences = [s['site'] for s in sites]
        pwm = PWM.from_sequences(site_sequences)
        
        # Get consensus
        consensus = pwm.consensus()
        
        return {
            'motif': motif,
            'consensus': consensus,
            'total_distance': distance,
            'sites': sites,
            'pwm': pwm,
            'method': method
        }
