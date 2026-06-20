"""
Unit tests for scoring functions.
"""

import pytest
import math

from bio_motif_finder.score import BackgroundModel, InformationContent, MotifScorer
from bio_motif_finder.pwm import PWM


class TestBackgroundModel:
    """Tests for background model."""
    
    def test_uniform_background(self):
        """Test uniform background model."""
        bg = BackgroundModel()
        
        for nuc in ['A', 'C', 'G', 'T']:
            assert bg.get_probability(nuc) == pytest.approx(0.25)
    
    def test_custom_background(self):
        """Test custom background model."""
        bg = BackgroundModel({'A': 0.3, 'C': 0.2, 'G': 0.2, 'T': 0.3})
        
        assert bg.get_probability('A') == pytest.approx(0.3)
        assert bg.get_probability('C') == pytest.approx(0.2)
    
    def test_custom_background_normalization(self):
        """Test that custom background is normalized."""
        bg = BackgroundModel({'A': 3, 'C': 2, 'G': 2, 'T': 3})
        
        total = sum(bg.get_probability(nuc) for nuc in ['A', 'C', 'G', 'T'])
        assert total == pytest.approx(1.0)
    
    def test_log_probability(self, background_uniform):
        """Test log probability calculation."""
        log_prob = background_uniform.get_log_probability('A')
        
        expected = math.log(0.25)
        assert log_prob == pytest.approx(expected)
    
    def test_unknown_nucleotide(self, background_uniform):
        """Test unknown nucleotide returns 0 probability."""
        prob = background_uniform.get_probability('N')
        
        assert prob == 0.0
    
    def test_score_sequence(self, background_uniform):
        """Test sequence scoring."""
        score = background_uniform.score_sequence("ATCG")
        
        # With uniform background, each nucleotide contributes log(0.25)
        expected = 4 * math.log(0.25)
        assert score == pytest.approx(expected)
    
    def test_to_dict(self, background_uniform):
        """Test conversion to dictionary."""
        bg_dict = background_uniform.to_dict()
        
        assert isinstance(bg_dict, dict)
        assert len(bg_dict) == 4
        assert all(nuc in bg_dict for nuc in ['A', 'C', 'G', 'T'])
    
    def test_from_sequences(self):
        """Test creation from sequences."""
        sequences = ["AAAATTT", "AAAATTT", "CCGGGGG"]
        bg = BackgroundModel.from_sequences(sequences)
        
        # A: 8/21, T: 4/21, C: 4/21, G: 5/21
        assert bg.get_probability('A') > bg.get_probability('C')


class TestInformationContent:
    """Tests for information content calculation."""
    
    def test_position_ic_conserved(self, ic_calculator):
        """Test IC for conserved position."""
        counts = {'A': 10, 'C': 0, 'G': 0, 'T': 0}
        
        ic = ic_calculator.position_ic(counts, 10)
        
        # Fully conserved position should have high IC (close to 2 bits)
        assert ic > 1.5
    
    def test_position_ic_variable(self, ic_calculator):
        """Test IC for variable position."""
        counts = {'A': 2, 'C': 3, 'G': 3, 'T': 2}
        
        ic = ic_calculator.position_ic(counts, 10)
        
        # Variable position should have low IC
        assert ic < 1.0
    
    def test_position_ic_uniform(self, ic_calculator):
        """Test IC for uniform distribution."""
        counts = {'A': 2, 'C': 3, 'G': 3, 'T': 2}
        
        ic = ic_calculator.position_ic(counts, 10)
        
        # Near-uniform should have IC close to 0
        assert ic < 0.5
    
    def test_motif_ic(self, ic_calculator):
        """Test total motif IC calculation."""
        counts_matrix = [
            {'A': 10, 'C': 0, 'G': 0, 'T': 0},  # Conserved
            {'A': 2, 'C': 3, 'G': 3, 'T': 2},  # Variable
            {'A': 10, 'C': 0, 'G': 0, 'T': 0},  # Conserved
        ]
        
        total_ic = ic_calculator.motif_ic(counts_matrix, 10)
        
        # Should be sum of position ICs
        assert total_ic > 3.0
    
    def test_relative_entropy(self, ic_calculator):
        """Test relative entropy calculation."""
        # KL divergence of identical distributions should be 0
        kl = ic_calculator.relative_entropy(0.25, 0.25)
        assert kl == pytest.approx(0.0)
        
        # KL divergence should be positive
        kl = ic_calculator.relative_entropy(0.5, 0.25)
        assert kl > 0


class TestMotifScorer:
    """Tests for comprehensive motif scorer."""
    
    def test_log_odds_calculation(self, scorer, sample_pwm):
        """Test log-odds score calculation."""
        log_odds = scorer.calculate_log_odds(sample_pwm)
        
        assert log_odds.shape == (4, 8)
        
        # Log-odds should be positive for favored nucleotides
        # and negative for disfavored
        assert log_odds[0, 0] > 0  # A at position 0 (favorable)
        assert log_odds[1, 0] < 0  # C at position 0 (disfavored)
    
    def test_score_site(self, scorer, sample_pwm):
        """Test site scoring."""
        # Perfect match should have positive score
        score = scorer.score_site(sample_pwm, "ATCGATCG")
        
        assert score > 0
        
        # Mismatched site should have lower score
        score_mismatch = scorer.score_site(sample_pwm, "GGGGGGGG")
        
        assert score_mismatch < score
    
    def test_scan_sequence(self, scorer, sample_pwm):
        """Test sequence scanning."""
        sequence = "ATCGATCGATCGATCG"
        
        matches = scorer.scan_sequence(sample_pwm, sequence, threshold=0.0)
        
        # Should find multiple matches
        assert len(matches) > 0
        
        # All matches should have positive scores
        for pos, score in matches:
            assert score > 0
    
    def test_scan_sequence_no_matches(self, scorer, sample_pwm):
        """Test scanning with no matches above threshold."""
        sequence = "GGGGGGGG"
        
        matches = scorer.scan_sequence(sample_pwm, sequence, threshold=100.0)
        
        # No matches should exceed very high threshold
        assert len(matches) == 0
    
    def test_consensus_score(self, scorer, sample_pwm):
        """Test consensus scoring."""
        consensus = sample_pwm.consensus()
        
        score = scorer.consensus_score(sample_pwm, consensus)
        
        # Consensus should score well
        assert score > 0


class TestScoringEdgeCases:
    """Tests for scoring edge cases."""
    
    def test_empty_sequence(self, scorer, sample_pwm):
        """Test scoring empty sequence."""
        matches = scorer.scan_sequence(sample_pwm, "", threshold=0.0)
        
        assert len(matches) == 0
    
    def test_short_sequence(self, scorer, sample_pwm):
        """Test scoring sequence shorter than PWM."""
        matches = scorer.scan_sequence(sample_pwm, "AT", threshold=0.0)
        
        assert len(matches) == 0
    
    def test_unknown_nucleotides(self, scorer, sample_pwm):
        """Test scoring sequence with unknown nucleotides."""
        score = scorer.score_site(sample_pwm, "NNNNNNNN")
        
        # Should handle gracefully
        assert score == -float('inf')
    
    def test_pwm_probability_sum(self, sample_pwm):
        """Test that probabilities sum to 1 at each position."""
        for j in range(sample_pwm.length):
            total = sum(sample_pwm.probabilities[j].values())
            assert abs(total - 1.0) < 0.001
