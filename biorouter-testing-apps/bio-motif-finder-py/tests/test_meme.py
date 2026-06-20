"""
Unit tests for MEME-lite algorithm.
"""

import pytest

from bio_motif_finder.meme import MEMELite, MEMEParser
from bio_motif_finder.pwm import PWM
from bio_motif_finder.score import BackgroundModel


class TestMEMELite:
    """Tests for MEME-lite algorithm."""
    
    def test_initialization(self):
        """Test MEME-lite initialization."""
        meme = MEMELite(motif_width=8, max_iterations=50)
        
        assert meme.motif_width == 8
        assert meme.max_iterations == 50
    
    def test_initialize_pwm(self):
        """Test PWM initialization."""
        meme = MEMELite(motif_width=4)
        sequences = ["ATCGATCG"] * 10
        
        pwm = meme._initialize_pwm(sequences, seed=42)
        
        assert pwm.length == 4
    
    def test_e_step(self):
        """Test E-step."""
        meme = MEMELite(motif_width=4)
        sequences = ["ATCGATCG"] * 5
        
        pwm = PWM.from_sequences(["ATCG"] * 5)
        posteriors = meme._e_step(sequences, pwm)
        
        assert len(posteriors) == 5
        assert len(posteriors[0]) == 5  # Number of sites
        
        # Probabilities should sum to ~1
        for seq_posteriors in posteriors:
            assert abs(sum(seq_posteriors) - 1.0) < 0.01
    
    def test_m_step(self):
        """Test M-step."""
        meme = MEMELite(motif_width=4)
        sequences = ["ATCGATCG"] * 5
        
        # Create posteriors with high probability at position 0
        posteriors = []
        for _ in sequences:
            probs = [0.9] + [0.025] * 4
            posteriors.append(probs)
        
        new_pwm = meme._m_step(sequences, posteriors)
        
        assert new_pwm.length == 4
        # Should reflect the posteriors
        consensus = new_pwm.consensus()
        assert consensus == "ATCG"
    
    def test_calculate_likelihood(self):
        """Test likelihood calculation."""
        meme = MEMELite(motif_width=4)
        sequences = ["ATCGATCG"] * 5
        
        pwm = PWM.from_sequences(["ATCG"] * 5)
        ll = meme._calculate_likelihood(sequences, pwm)
        
        # Likelihood should be a finite number
        assert -float('inf') < ll < float('inf')
    
    def test_run(self):
        """Test single run."""
        meme = MEMELite(motif_width=4, max_iterations=20)
        sequences = [
            "XXXATCGXXX",
            "XXXATCGXXX",
            "XXXATCGXXX"
        ]
        
        result = meme.run(sequences, seed=42)
        
        assert 'consensus' in result
        assert 'sites' in result
        assert 'pwm' in result
        assert 'log_likelihood' in result
        assert result['method'] == 'meme'
    
    def test_find_motif(self):
        """Test motif finding with multiple starts."""
        meme = MEMELite(motif_width=4, max_iterations=20)
        sequences = [
            "XXXATCGXXX",
            "XXXATCGXXX",
            "XXXATCGXXX"
        ]
        
        result = meme.find_motif(sequences, num_starts=3, seed=42)
        
        assert result['consensus'] == "ATCG"


class TestMEMEMotifRecovery:
    """Tests for motif recovery in planted data."""
    
    def test_recovers_simple_motif(self):
        """Test recovery of simple motif."""
        from bio_motif_finder.simulate import MotifSimulator
        
        simulator = MotifSimulator(seed=42)
        data = simulator.generate_dataset(
            num_sequences=15,
            sequence_length=80,
            motif_length=8,
            motif="ATCGATCG",
            mutations_per_instance=1
        )
        
        meme = MEMELite(motif_width=8, max_iterations=50)
        result = meme.find_motif(data.sequences, num_starts=5, seed=42)
        
        # Calculate Hamming distance
        consensus = result['consensus']
        hamming = sum(c1 != c2 for c1, c2 in zip(consensus, data.motif))
        
        # Should be reasonably close
        assert hamming <= 3
    
    def test_increases_likelihood(self):
        """Test that likelihood increases during EM."""
        meme = MEMELite(motif_width=4, max_iterations=30)
        sequences = ["ATCGATCG"] * 10
        
        # Track likelihoods
        result1 = meme.run(sequences, seed=42)
        
        # Run with more iterations
        meme2 = MEMELite(motif_width=4, max_iterations=100)
        result2 = meme2.run(sequences, seed=42)
        
        # More iterations should generally improve likelihood
        assert result2['log_likelihood'] >= result1['log_likelihood']


class TestMEMEParser:
    """Tests for MEME output formatter."""
    
    def test_format_results(self):
        """Test results formatting."""
        result = {
            'consensus': "ATCG",
            'sites': [
                {'sequence_index': 0, 'position': 10, 'site': 'ATCG'},
                {'sequence_index': 1, 'position': 20, 'site': 'ATCG'}
            ]
        }
        
        formatted = MEMEParser.format_results(result)
        
        assert "MEME version" in formatted
        assert "MOTIF 1 ATCG" in formatted
    
    def test_format_with_sequences(self):
        """Test formatting with sequences."""
        result = {
            'consensus': "ATCG",
            'sites': [
                {'sequence_index': 0, 'position': 10, 'site': 'ATCG'}
            ]
        }
        sequences = ["A" * 20]
        
        formatted = MEMEParser.format_results(result, sequences)
        
        assert "A" * 20 in formatted
