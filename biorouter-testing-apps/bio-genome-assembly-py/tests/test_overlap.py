"""
Tests for the overlap detection module.
"""

import pytest

from bio_assembly.io import SequenceRecord
from bio_assembly.overlap import (
    Overlap,
    build_overlap_graph,
    find_overlaps,
    hamming_distance,
    prefix_suffix_overlap_length,
    transitive_reduction,
)


class TestHammingDistance:
    """Tests for Hamming distance calculation."""
    
    def test_identical_strings(self):
        """Test Hamming distance of identical strings."""
        assert hamming_distance("AAAA", "AAAA") == 0
    
    def test_completely_different(self):
        """Test Hamming distance of completely different strings."""
        assert hamming_distance("AAAA", "TTTT") == 4
    
    def test_partial_mismatch(self):
        """Test Hamming distance with partial mismatches."""
        assert hamming_distance("ACGT", "ACCT") == 1
        assert hamming_distance("ACGT", "TCGT") == 1
    
    def test_unequal_length_raises(self):
        """Test that unequal lengths raise ValueError."""
        with pytest.raises(ValueError):
            hamming_distance("AAA", "AAAA")


class TestPrefixSuffixOverlap:
    """Tests for prefix-suffix overlap detection."""
    
    def test_exact_overlap(self):
        """Test detection of exact overlap."""
        read_a = "ACGTACGT"
        read_b = "ACGTTTTT"
        
        # Suffix of A: "ACGT" matches prefix of B: "ACGT"
        overlap_len = prefix_suffix_overlap_length(read_a, read_b)
        assert overlap_len == 4
    
    def test_longer_overlap(self):
        """Test detection of longer overlap."""
        read_a = "AAAACCCCGGGG"
        read_b = "CCCCGGGGTTTT"
        
        # Overlap is "CCCCGGGG"
        overlap_len = prefix_suffix_overlap_length(read_a, read_b)
        assert overlap_len == 8
    
    def test_no_overlap(self):
        """Test when there is no overlap."""
        read_a = "AAAA"
        read_b = "TTTT"
        
        overlap_len = prefix_suffix_overlap_length(read_a, read_b)
        assert overlap_len is None
    
    def test_full_overlap(self):
        """Test when one read is fully contained in overlap."""
        read_a = "ACGT"
        read_b = "ACGTACGT"
        
        # Suffix of A matches prefix of B up to length of A
        overlap_len = prefix_suffix_overlap_length(read_a, read_b)
        assert overlap_len == 4
    
    def test_error_tolerance(self):
        """Test overlap detection with errors."""
        read_a = "AAAAACGT"
        read_b = "ACGTTTTT"
        
        # "ACGT" matches with 0 errors
        overlap_len = prefix_suffix_overlap_length(read_a, read_b, max_errors=0)
        assert overlap_len == 4
        
        # "AACGT" matches with 1 error (A vs C at pos 1)
        read_a2 = "AAAAACGT"
        read_b2 = "AACGTTTT"
        overlap_len2 = prefix_suffix_overlap_length(read_a2, read_b2, max_errors=1)
        assert overlap_len2 >= 4


class TestFindOverlaps:
    """Tests for finding overlaps between reads."""
    
    def test_simple_overlap(self):
        """Test finding overlaps between simple reads."""
        reads = [
            SequenceRecord("r1", "", "AAAAACGT"),
            SequenceRecord("r2", "", "ACGTTTTT"),
            SequenceRecord("r3", "", "TTTTAAAA"),
        ]
        
        overlaps = find_overlaps(reads, min_overlap=4, max_errors=0)
        
        # Should find r1->r2 overlap of length 4
        r1_r2_overlaps = [o for o in overlaps if o.read_a == 0 and o.read_b == 1]
        assert len(r1_r2_overlaps) >= 1
        assert r1_r2_overlaps[0].length == 4
    
    def test_multiple_overlaps(self):
        """Test finding multiple overlaps."""
        reads = [
            SequenceRecord("r1", "", "AAAAAAAA"),
            SequenceRecord("r2", "", "AAAACCCC"),
            SequenceRecord("r3", "", "CCCCGGGG"),
        ]
        
        overlaps = find_overlaps(reads, min_overlap=4, max_errors=0)
        
        # Should find r1->r2 and r2->r3
        assert any(o.read_a == 0 and o.read_b == 1 for o in overlaps)
        assert any(o.read_a == 1 and o.read_b == 2 for o in overlaps)
    
    def test_no_overlaps(self):
        """Test when there are no overlaps."""
        reads = [
            SequenceRecord("r1", "", "ACGT"),
            SequenceRecord("r2", "", "TGCA"),
            SequenceRecord("r3", "", "GGGG"),
        ]
        
        # Use both_strands=False to avoid reverse complement matching
        overlaps = find_overlaps(reads, min_overlap=4, max_errors=0, both_strands=False)
        assert len(overlaps) == 0
    
    def test_max_reads_limit(self):
        """Test max_reads limiting."""
        reads = [
            SequenceRecord("r1", "", "ACGTACGT"),
            SequenceRecord("r2", "", "ACGTTTTT"),
            SequenceRecord("r3", "", "TTTTAAAA"),
        ]
        
        overlaps = find_overlaps(reads, min_overlap=4, max_errors=0, max_reads=2)
        
        # Only first 2 reads should be processed
        assert all(o.read_a < 2 and o.read_b < 2 for o in overlaps)


class TestBuildOverlapGraph:
    """Tests for building overlap graph."""
    
    def test_graph_structure(self):
        """Test that graph is built correctly."""
        reads = [
            SequenceRecord("r1", "", "AAAAACGT"),
            SequenceRecord("r2", "", "ACGTTTTT"),
            SequenceRecord("r3", "", "TTTTAAAA"),
        ]
        
        overlaps = find_overlaps(reads, min_overlap=4, max_errors=0)
        graph = build_overlap_graph(reads, overlaps)
        
        assert 0 in graph
        assert 1 in graph
        assert 2 in graph
        
        # r1 should have edge to r2
        assert any(o.read_b == 1 for o in graph[0])


class TestTransitiveReduction:
    """Tests for transitive reduction."""
    
    def test_removes_transitive_edges(self):
        """Test that transitive edges are removed."""
        # Create overlaps: 0->1, 1->2, 0->2 (transitive)
        overlaps = [
            Overlap(0, 1, 0, 10, 1.0, False),
            Overlap(1, 2, 5, 10, 1.0, False),
            Overlap(0, 2, 5, 15, 1.0, False),  # Transitive
        ]
        
        reduced = transitive_reduction(overlaps)
        
        # 0->2 should be removed
        assert not any(o.read_a == 0 and o.read_b == 2 for o in reduced)
        # 0->1 and 1->2 should remain
        assert any(o.read_a == 0 and o.read_b == 1 for o in reduced)
        assert any(o.read_a == 1 and o.read_b == 2 for o in reduced)
    
    def test_keeps_non_transitive(self):
        """Test that non-transitive edges are kept."""
        overlaps = [
            Overlap(0, 1, 0, 10, 1.0, False),
            Overlap(0, 2, 0, 15, 1.0, False),
        ]
        
        reduced = transitive_reduction(overlaps)
        
        # Both should remain
        assert len(reduced) == 2
