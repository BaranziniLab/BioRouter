"""
Unit tests for Position Weight Matrix (PWM).
"""

import pytest
from collections import Counter

from bio_motif_finder.pwm import PWM, PWMSet


class TestPWMCreation:
    """Tests for PWM creation methods."""
    
    def test_from_sequences(self, sample_sequences):
        """Test PWM creation from aligned sequences."""
        pwm = PWM.from_sequences(sample_sequences, pseudocount=0.1)
        
        assert pwm.length == 8
        assert len(pwm.probabilities) == 8
        
        # All sequences identical, so each position should have high probability for dominant nucleotide
        for j in range(pwm.length):
            probs = pwm.probabilities[j]
            max_prob = max(probs.values())
            assert max_prob > 0.9  # Should be close to 1.0
    
    def test_from_sequences_with_pseudocount(self, sample_sequences):
        """Test PWM creation with pseudocounts."""
        pwm = PWM.from_sequences(sample_sequences, pseudocount=0.1)
        
        # With small pseudocount, still should have high probability for dominant nucleotide
        for j in range(pwm.length):
            probs = pwm.probabilities[j]
            max_prob = max(probs.values())
            assert max_prob > 0.9
    
    def test_from_counts(self):
        """Test PWM creation from explicit counts."""
        counts = [
            {'A': 10, 'C': 0, 'G': 0, 'T': 0},
            {'A': 0, 'C': 10, 'G': 0, 'T': 0},
            {'A': 0, 'C': 0, 'G': 10, 'T': 0},
            {'A': 0, 'C': 0, 'G': 0, 'T': 10},
        ]
        
        pwm = PWM.from_counts(counts)
        
        assert pwm.length == 4
        # Check probabilities sum to ~1
        for j in range(pwm.length):
            total = sum(pwm.probabilities[j].values())
            assert abs(total - 1.0) < 0.01
    
    def test_empty_sequences_raises(self):
        """Test that empty sequences raise ValueError."""
        with pytest.raises(ValueError):
            PWM.from_sequences([])
    
    def test_misaligned_sequences_raises(self):
        """Test that misaligned sequences raise ValueError."""
        sequences = ["ATCG", "ATC", "ATCGAT"]
        with pytest.raises(ValueError):
            PWM.from_sequences(sequences)
    
    def test_random_pwm(self):
        """Test random PWM generation."""
        pwm = PWM.random(10)
        
        assert pwm.length == 10
        assert len(pwm.probabilities) == 10
        
        # Each position should sum to ~1
        for j in range(pwm.length):
            total = sum(pwm.probabilities[j].values())
            assert abs(total - 1.0) < 0.01


class TestPWMProperties:
    """Tests for PWM properties and methods."""
    
    def test_get_probability(self, sample_pwm):
        """Test probability retrieval."""
        # Get probability of A at position 0 (should be high for ATCGATCG)
        prob_a = sample_pwm.get_probability('A', 0)
        prob_c = sample_pwm.get_probability('C', 0)
        
        assert prob_a > prob_c
    
    def test_get_probability_invalid_position(self, sample_pwm):
        """Test invalid position raises IndexError."""
        with pytest.raises(IndexError):
            sample_pwm.get_probability('A', 100)
    
    def test_get_counts(self):
        """Test count retrieval."""
        # Use sequences with all nucleotides
        sequences = ["ACGT", "ACGT", "ACGT"]
        pwm = PWM.from_sequences(sequences, pseudocount=0.0)
        counts = pwm.get_counts(0)
        
        assert isinstance(counts, dict)
        assert counts['A'] == 3
        # Counts dict only contains nucleotides that were observed
        # Position 0 has only 'A' in these sequences
        assert 'C' not in counts or counts['C'] == 0
    
    def test_consensus(self, sample_pwm):
        """Test consensus extraction."""
        consensus = sample_pwm.consensus()
        
        assert len(consensus) == 8
        # For identical sequences, consensus should match
        assert consensus == "ATCGATCG"
    
    def test_weblogo_data(self, sample_pwm):
        """Test weblogo data generation."""
        logo_data = sample_pwm.weblogo_data()
        
        assert len(logo_data) == 8
        
        for j in range(8):
            assert j in logo_data
            assert len(logo_data[j]) == 4
            
            # Heights should sum to information content
            total_height = sum(logo_data[j].values())
            assert total_height >= 0
    
    def test_pwm_length(self, sample_pwm):
        """Test PWM length property."""
        assert len(sample_pwm) == 8
    
    def test_pwm_repr(self, sample_pwm):
        """Test string representation."""
        repr_str = repr(sample_pwm)
        assert "PWM" in repr_str
        assert "length=8" in repr_str


class TestPWMOperations:
    """Tests for PWM operations."""
    
    def test_similarity_identical(self, sample_pwm):
        """Test similarity of identical PWMs."""
        similarity = sample_pwm.similarity(sample_pwm)
        
        # Identical PWMs should have similarity ~1.0
        assert similarity > 0.99
    
    def test_similarity_different(self):
        """Test similarity of different PWMs."""
        # Create very different PWMs with no pseudocounts
        at_sequences = ["ATATATAT"] * 10
        gc_sequences = ["GCGCGCGC"] * 10
        
        at_pwm = PWM.from_sequences(at_sequences, pseudocount=0.0)
        gc_pwm = PWM.from_sequences(gc_sequences, pseudocount=0.0)
        
        similarity = at_pwm.similarity(gc_pwm)
        
        # Completely different PWMs should have very low similarity
        assert similarity < 0.3
    
    def test_reverse_complement(self):
        """Test reverse complement generation."""
        sequences = ["ATCGATCG"]
        pwm = PWM.from_sequences(sequences)
        
        rc_pwm = pwm.reverse_complement()
        
        assert rc_pwm.length == pwm.length
        
        # Reverse complement of ATCG is CGAT
        # So reverse complement of ATCGATCG is CGATCGAT
        rc_consensus = rc_pwm.consensus()
        assert rc_consensus == "CGATCGAT"
    
    def test_trim(self, sample_pwm):
        """Test PWM trimming."""
        trimmed = sample_pwm.trim(0, 4)
        
        assert trimmed.length == 4
        
        # Consensus should be first 4 bases
        consensus = trimmed.consensus()
        assert consensus == "ATCG"


class TestPWMSet:
    """Tests for PWMSet class."""
    
    def test_add_pwm(self):
        """Test adding PWMs to set."""
        pwm_set = PWMSet()
        
        pwm1 = PWM.random(8)
        pwm2 = PWM.random(8)
        
        pwm_set.add(pwm1, "motif1")
        pwm_set.add(pwm2, "motif2")
        
        assert len(pwm_set.pwms) == 2
        assert len(pwm_set.names) == 2
    
    def test_empty_set_raises(self):
        """Test that empty set raises ValueError."""
        from bio_motif_finder.score import MotifScorer
        
        pwm_set = PWMSet()
        scorer = MotifScorer()
        
        with pytest.raises(ValueError):
            pwm_set.get_best(scorer)
