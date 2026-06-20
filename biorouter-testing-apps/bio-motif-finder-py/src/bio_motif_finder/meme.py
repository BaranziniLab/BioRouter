"""
MEME-lite: EM-style motif discovery algorithm.

Implements an Expectation-Maximization approach for finding motifs,
building Position Weight Matrices iteratively.
"""

import random
import math
from typing import List, Optional, Tuple, Dict
from collections import Counter

import numpy as np

from bio_motif_finder.pwm import PWM
from bio_motif_finder.score import BackgroundModel, MotifScorer


class MEMELite:
    """
    MEME-lite: EM-style algorithm for motif discovery.
    
    Uses expectation-maximization to find motifs and build PWMs.
    """
    
    NUCLEOTIDES = ['A', 'C', 'G', 'T']
    
    def __init__(self,
                 motif_width: int = 8,
                 num_motifs: int = 1,
                 max_iterations: int = 100,
                 convergence_threshold: float = 1e-6,
                 background: Optional[BackgroundModel] = None,
                 pseudocount: float = 1.0):
        """
        Initialize MEME-lite.
        
        Args:
            motif_width: Width of motifs to find.
            num_motifs: Number of motifs to discover.
            max_iterations: Maximum EM iterations.
            convergence_threshold: Convergence threshold.
            background: Background model.
            pseudocount: Pseudocount for smoothing.
        """
        self.motif_width = motif_width
        self.num_motifs = num_motifs
        self.max_iterations = max_iterations
        self.convergence_threshold = convergence_threshold
        self.background = background or BackgroundModel()
        self.pseudocount = pseudocount
        self.scorer = MotifScorer(self.background)
    
    def _initialize_pwm(self, sequences: List[str], seed: Optional[int] = None) -> PWM:
        """
        Initialize PWM from random sites.
        
        Args:
            sequences: List of sequences.
            seed: Random seed.
        
        Returns:
            Initial PWM.
        """
        if seed is not None:
            random.seed(seed)
        
        site_sequences = []
        
        for seq in sequences:
            max_pos = len(seq) - self.motif_width
            pos = random.randint(0, max_pos)
            site = seq[pos:pos + self.motif_width]
            site_sequences.append(site.upper())
        
        return PWM.from_sequences(site_sequences, self.pseudocount)
    
    def _e_step(self, 
                sequences: List[str], 
                pwm: PWM) -> List[List[float]]:
        """
        E-step: Calculate posterior probabilities for each site.
        
        Args:
            sequences: List of sequences.
            pwm: Current PWM.
        
        Returns:
            Matrix of posterior probabilities.
        """
        posteriors = []
        
        for seq in sequences:
            seq_upper = seq.upper()
            n_sites = len(seq_upper) - self.motif_width + 1
            
            # Calculate log-odds for each site
            scores = []
            for i in range(n_sites):
                site = seq_upper[i:i + self.motif_width]
                score = self.scorer.score_site(pwm, site)
                scores.append(score)
            
            # Convert to probabilities using softmax
            max_score = max(scores) if scores else 0
            exp_scores = [math.exp(s - max_score) for s in scores]
            total = sum(exp_scores)
            
            if total > 0:
                probs = [s / total for s in exp_scores]
            else:
                probs = [1.0 / n_sites] * n_sites
            
            posteriors.append(probs)
        
        return posteriors
    
    def _m_step(self, 
                sequences: List[str], 
                posteriors: List[List[float]]) -> PWM:
        """
        M-step: Update PWM from posterior probabilities.
        
        Args:
            sequences: List of sequences.
            posteriors: Posterior probability matrix.
        
        Returns:
            Updated PWM.
        """
        # Calculate expected counts
        counts_matrix = []
        
        for j in range(self.motif_width):
            position_counts = {nuc: 0.0 for nuc in self.NUCLEOTIDES}
            
            for seq, seq_posteriors in zip(sequences, posteriors):
                seq_upper = seq.upper()
                
                for i in range(len(seq_upper) - self.motif_width + 1):
                    site = seq_upper[i:i + self.motif_width]
                    nuc = site[j]
                    
                    if nuc in self.NUCLEOTIDES:
                        position_counts[nuc] += seq_posteriors[i]
            
            # Convert to integers (with pseudocounts)
            int_counts = {nuc: max(1, int(count + 0.5)) for nuc, count in position_counts.items()}
            counts_matrix.append(int_counts)
        
        return PWM.from_counts(counts_matrix, self.pseudocount)
    
    def _calculate_likelihood(self, 
                             sequences: List[str], 
                             pwm: PWM) -> float:
        """
        Calculate log-likelihood of data given PWM.
        
        Args:
            sequences: List of sequences.
            pwm: Current PWM.
        
        Returns:
            Log-likelihood.
        """
        total_ll = 0.0
        
        for seq in sequences:
            seq_upper = seq.upper()
            n_sites = len(seq_upper) - self.motif_width + 1
            
            # Sum of probabilities across sites
            site_probs = []
            for i in range(n_sites):
                site = seq_upper[i:i + self.motif_width]
                
                # Probability of site under PWM vs background
                log_odds = 0.0
                for j, nuc in enumerate(site):
                    if nuc in self.NUCLEOTIDES:
                        pwm_prob = pwm.get_probability(nuc, j)
                        bg_prob = self.background.get_probability(nuc)
                        
                        if pwm_prob > 0 and bg_prob > 0:
                            log_odds += math.log(pwm_prob / bg_prob)
                
                site_probs.append(math.exp(log_odds))
            
            # Log sum of site probabilities
            total_ll += math.log(sum(site_probs) + 1e-10)
        
        return total_ll
    
    def run(self, 
           sequences: List[str],
           seed: Optional[int] = None) -> Dict:
        """
        Run MEME-lite algorithm.
        
        Args:
            sequences: List of sequences.
            seed: Random seed.
        
        Returns:
            Dictionary with results.
        """
        if seed is not None:
            random.seed(seed)
            np.random.seed(seed)
        
        # Initialize PWM
        pwm = self._initialize_pwm(sequences, seed)
        
        best_pwm = pwm
        best_ll = -float('inf')
        
        # EM iterations
        for iteration in range(self.max_iterations):
            # E-step
            posteriors = self._e_step(sequences, pwm)
            
            # M-step
            new_pwm = self._m_step(sequences, posteriors)
            
            # Calculate likelihood
            ll = self._calculate_likelihood(sequences, new_pwm)
            
            # Track best
            if ll > best_ll:
                best_ll = ll
                best_pwm = new_pwm
            
            # Check convergence
            if iteration > 0 and abs(ll - best_ll) < self.convergence_threshold:
                break
            
            pwm = new_pwm
        
        # Extract results
        sites = []
        site_sequences = []
        
        for i, seq in enumerate(sequences):
            seq_upper = seq.upper()
            best_pos = 0
            best_score = -float('inf')
            
            # Find best site in this sequence
            for j in range(len(seq_upper) - self.motif_width + 1):
                site = seq_upper[j:j + self.motif_width]
                score = self.scorer.score_site(best_pwm, site)
                
                if score > best_score:
                    best_score = score
                    best_pos = j
            
            site = seq_upper[best_pos:best_pos + self.motif_width]
            site_sequences.append(site)
            
            sites.append({
                'sequence_index': i,
                'position': best_pos,
                'site': site,
                'score': best_score
            })
        
        # Build final PWM
        final_pwm = PWM.from_sequences(site_sequences, self.pseudocount)
        consensus = final_pwm.consensus()
        
        return {
            'motif': consensus,
            'consensus': consensus,
            'sites': sites,
            'pwm': final_pwm,
            'log_likelihood': best_ll,
            'iterations': iteration + 1,
            'method': 'meme'
        }
    
    def find_motif(self,
                  sequences: List[str],
                  num_starts: int = 5,
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
        best_ll = -float('inf')
        
        for i in range(num_starts):
            current_seed = (seed + i) if seed is not None else None
            result = self.run(sequences, seed=current_seed)
            
            if result['log_likelihood'] > best_ll:
                best_ll = result['log_likelihood']
                best_result = result
        
        return best_result


class MEMEParser:
    """
    Parser for MEME output format.
    """
    
    @staticmethod
    def format_results(result: Dict, 
                      sequences: Optional[List[str]] = None) -> str:
        """
        Format results in MEME-like output format.
        
        Args:
            result: Algorithm results.
            sequences: Original sequences.
        
        Returns:
            Formatted string.
        """
        lines = []
        lines.append("MEME version 4.0")
        lines.append("")
        lines.append("ALPHABET= ACGT")
        lines.append("")
        lines.append(f"strands: + -")
        lines.append(f"Background letter frequencies:")
        lines.append("A 0.25 C 0.25 G 0.25 T 0.25")
        lines.append("")
        lines.append(f"MOTIF 1 {result['consensus']}")
        lines.append(f"width={len(result['consensus'])}  sites={len(result['sites'])}")
        lines.append("")
        
        if sequences:
            for i, (seq, site_info) in enumerate(zip(sequences, result['sites'])):
                lines.append(f"  {i+1:2d} {seq[:50]:50s}  {site_info['position']:3d}  {site_info['site']}")
        
        return '\n'.join(lines)
