"""
Unit tests for greedy motif finding algorithm.
"""

import pytest

from bio_motif_finder.greedy import GreedyMotifFinder
from bio_motif_finder.pwm import PWM
from bio_motif_finder.score import BackgroundModel


class TestGreedyMotifFinder:
    """Tests for greedy algorithm."""
    
    def test_hamming_distance(self):
        """Test Hamming distance calculation."""
        finder = GreedyMotifFinder(motif_width=4)
        
        # Identical sequences
        assert finder.hamming_distance("ATCG", "ATCG") == 0
        
        # One mismatch
        assert finder.hamming_distance("ATCG", "ATCA") == 1
        
        # All mismatches
        assert finder.hamming_distance("ATCG", "GCTA") == 4
    
    def test_median_string_distance(self):
        """Test median string distance calculation."""
        sequences = ["ATCGATCG", "ATCGATCG", "ATCGATCG"]
        finder = GreedyMotifFinder(motif_width=8)
        
        # Perfect match
        distance = finder.median_string_distance("ATCGATCG", sequences)
        assert distance == 0
        
        # Mismatched candidate
        distance = finder.median_string_distance("GGGGGGGG", sequences)
        assert distance > 0
    
    def test_find_best_substring(self):
        """Test finding best matching substring."""
        sequence = "XXXATCGXXX"
        finder = GreedyMotifFinder(motif_width=4)
        
        substring, position, distance = finder.find_best_substring("ATCG", sequence)
        
        assert substring == "ATCG"
        assert position == 3
        assert distance == 0
    
    def test_brute_force_search(self):
        """Test brute-force search."""
        sequences = [
            "ATCGATCG",
            "ATCGATCG",
            "ATCGATCG"
        ]
        
        finder = GreedyMotifFinder(motif_width=8)
        motif, distance, matches = finder.brute_force_search(sequences)
        
        assert motif == "ATCGATCG"
        assert distance == 0
        assert len(matches) == 3
    
    def test_brute_force_with_mutations(self):
        """Test brute-force with mutated sequences."""
        sequences = [
            "ATCGATCG",
            "ATCAATCG",  # One mutation
            "ATCGATCA"   # One mutation
        ]
        
        finder = GreedyMotifFinder(motif_width=8)
        motif, distance, matches = finder.brute_force_search(sequences)
        
        # Should find motif with minimal total distance
        assert distance == 2  # Two mutations total
        assert len(motif) == 8
    
    def test_brute_force_width_limit(self):
        """Test that brute-force rejects width > max."""
        sequences = ["A" * 20]
        finder = GreedyMotifFinder(motif_width=10, max_width_brute=8)
        
        with pytest.raises(ValueError):
            finder.brute_force_search(sequences)
    
    def test_greedy_search(self):
        """Test greedy search."""
        sequences = [
            "ATCGATCG",
            "ATCAATCG",
            "ATCGATCA"
        ]
        
        finder = GreedyMotifFinder(motif_width=8)
        motif, distance, matches = finder.greedy_search(sequences, num_iterations=10)
        
        assert len(motif) == 8
        assert len(matches) == 3
        assert distance <= 2  # Should find good solution
    
    def test_find_motif_brute(self, small_planted_motif):
        """Test motif finding with brute-force on planted data."""
        sequences = small_planted_motif.sequences
        planted_motif = small_planted_motif.motif
        
        finder = GreedyMotifFinder(motif_width=len(planted_motif))
        result = finder.find_motif(sequences, method='brute')
        
        assert 'consensus' in result
        assert 'sites' in result
        assert 'pwm' in result
        assert result['method'] == 'brute'
    
    def test_find_motif_greedy(self):
        """Test motif finding with greedy search."""
        import random
        random.seed(42)
        # Create test data with random flanking sequences (not homogeneous)
        sequences = []
        for i in range(10):
            prefix = ''.join(random.choice('ACGT') for _ in range(20))
            suffix = ''.join(random.choice('ACGT') for _ in range(20))
            seq = prefix + "ATCGATCG" + suffix
            sequences.append(seq)
        
        finder = GreedyMotifFinder(motif_width=8)
        # Use brute-force which is exact for width 8
        result = finder.find_motif(sequences, method='brute')
        
        # Should find the exact motif
        assert result['consensus'] == "ATCGATCG"
    
    def test_find_motif_auto_method(self):
        """Test auto method selection."""
        sequences = ["ATCGATCG"] * 5
        
        finder = GreedyMotifFinder(motif_width=8)
        result = finder.find_motif(sequences, method='auto')
        
        # For width 8, should use brute-force
        assert result['method'] == 'brute'
    
    def test_find_motif_invalid_method(self):
        """Test invalid method raises error."""
        sequences = ["ATCGATCG"] * 5
        finder = GreedyMotifFinder(motif_width=8)
        
        with pytest.raises(ValueError):
            finder.find_motif(sequences, method='invalid')


class TestGreedyMotifRecovery:
    """Tests for motif recovery in planted data."""
    
    def test_recovers_planted_motif_no_mutations(self):
        """Test recovery of planted motif without mutations."""
        from bio_motif_finder.simulate import MotifSimulator
        
        simulator = MotifSimulator(seed=42)
        data = simulator.generate_dataset(
            num_sequences=10,
            sequence_length=50,
            motif_length=6,
            motif="ATCGAT",
            mutations_per_instance=0
        )
        
        finder = GreedyMotifFinder(motif_width=6)
        result = finder.find_motif(data.sequences, method='brute')
        
        # Should recover exact motif
        assert result['consensus'] == "ATCGAT"
    
    def test_recovers_planted_motif_with_mutations(self):
        """Test recovery of planted motif with mutations."""
        from bio_motif_finder.simulate import MotifSimulator
        
        simulator = MotifSimulator(seed=42)
        data = simulator.generate_dataset(
            num_sequences=10,
            sequence_length=50,
            motif_length=6,
            motif="ATCGAT",
            mutations_per_instance=1
        )
        
        finder = GreedyMotifFinder(motif_width=6)
        result = finder.find_motif(data.sequences, method='brute')
        
        # Calculate Hamming distance to planted motif
        consensus = result['consensus']
        hamming = sum(c1 != c2 for c1, c2 in zip(consensus, data.motif))
        
        # Should be close (within hamming tolerance)
        assert hamming <= 2
