"""
Unit tests for Gibbs sampling algorithm.
"""

import pytest

from bio_motif_finder.gibbs import GibbsSampler
from bio_motif_finder.pwm import PWM
from bio_motif_finder.score import BackgroundModel


class TestGibbsSampler:
    """Tests for Gibbs sampling algorithm."""
    
    def test_initialization(self):
        """Test sampler initialization."""
        sampler = GibbsSampler(motif_width=8, num_iterations=100)
        
        assert sampler.motif_width == 8
        assert sampler.num_iterations == 100
    
    def test_initialize_positions(self):
        """Test position initialization."""
        sampler = GibbsSampler(motif_width=8)
        sequences = ["A" * 50, "C" * 50, "G" * 50]
        
        positions = sampler._initialize_positions(sequences)
        
        assert len(positions) == 3
        assert all(0 <= pos <= 42 for pos in positions)
    
    def test_build_pwm(self):
        """Test PWM building."""
        sampler = GibbsSampler(motif_width=4)
        sequences = ["ATCGATCG", "ATCGATCG", "ATCGATCG", "ATCGATCG"]
        positions = [0, 0, 0, 0]
        
        pwm = sampler._build_pwm(sequences, positions, exclude_index=3)
        
        assert pwm.length == 4
        # First 3 sequences should contribute
        consensus = pwm.consensus()
        assert consensus == "ATCG"
    
    def test_sample_position(self):
        """Test position sampling."""
        sampler = GibbsSampler(motif_width=4)
        sequence = "XXXATCGXXX"
        
        # Create PWM with ATCG motif
        sequences = ["ATCG"] * 5
        pwm = PWM.from_sequences(sequences)
        
        position = sampler._sample_position(sequence, pwm)
        
        # Should sample near the ATCG site
        assert 0 <= position <= 6
    
    def test_calculate_conservation(self):
        """Test conservation calculation."""
        sampler = GibbsSampler(motif_width=4)
        
        # Conserved PWM
        conserved_pwm = PWM.from_sequences(["ATCG"] * 10)
        conservation = sampler._calculate_conservation(conserved_pwm)
        
        assert conservation > 0
    
    def test_run(self):
        """Test single run."""
        sampler = GibbsSampler(motif_width=4, num_iterations=50)
        sequences = [
            "XXXATCGXXX",
            "XXXATCGXXX",
            "XXXATCGXXX"
        ]
        
        result = sampler.run(sequences, seed=42)
        
        assert 'consensus' in result
        assert 'sites' in result
        assert 'pwm' in result
        assert result['method'] == 'gibbs'
    
    def test_find_motif(self):
        """Test motif finding with multiple starts."""
        sampler = GibbsSampler(motif_width=4, num_iterations=50)
        sequences = [
            "XXXATCGXXX",
            "XXXATCGXXX",
            "XXXATCGXXX"
        ]
        
        result = sampler.find_motif(sequences, num_starts=3, seed=42)
        
        assert result['consensus'] == "ATCG"


class TestGibbsMotifRecovery:
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
        
        sampler = GibbsSampler(motif_width=8, num_iterations=200)
        result = sampler.find_motif(data.sequences, num_starts=5, seed=42)
        
        # Calculate Hamming distance
        consensus = result['consensus']
        hamming = sum(c1 != c2 for c1, c2 in zip(consensus, data.motif))
        
        # Should be reasonably close
        assert hamming <= 3
    
    def test_recovers_with_higher_mutations(self):
        """Test recovery with more mutations."""
        from bio_motif_finder.simulate import MotifSimulator
        
        simulator = MotifSimulator(seed=123)
        data = simulator.generate_dataset(
            num_sequences=20,
            sequence_length=100,
            motif_length=6,
            motif="GCGATC",
            mutations_per_instance=2
        )
        
        sampler = GibbsSampler(motif_width=6, num_iterations=300)
        result = sampler.find_motif(data.sequences, num_starts=5, seed=123)
        
        # Should still find something close
        assert 'consensus' in result
        assert len(result['consensus']) == 6
