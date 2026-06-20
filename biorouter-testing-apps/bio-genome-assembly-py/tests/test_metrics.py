"""
Tests for assembly metrics.
"""

import pytest

from bio_assembly.metrics import (
    AssemblyStats,
    compare_assemblies,
    compute_assembly_stats,
    compute_assembly_stats_from_records,
    compute_gc_content,
    compute_n50,
    count_gaps,
)
from bio_assembly.io import SequenceRecord


class TestComputeN50:
    """Tests for N50 computation."""
    
    def test_single_contig(self):
        """Test N50 with a single contig."""
        lengths = [1000]
        n50, l50 = compute_n50(lengths)
        assert n50 == 1000
        assert l50 == 1
    
    def test_equal_contigs(self):
        """Test N50 with equal-sized contigs."""
        lengths = [100, 100, 100, 100, 100]
        n50, l50 = compute_n50(lengths)
        assert n50 == 100
        assert l50 == 3  # Need 3 contigs to cover 50%
    
    def test_unequal_contigs(self):
        """Test N50 with unequal-sized contigs."""
        # Total = 1000, half = 500
        # Sorted: 500, 300, 200
        # After 500: 500/500 = 100% > 50%, so N50 = 500
        lengths = [200, 300, 500]
        n50, l50 = compute_n50(lengths)
        assert n50 == 500
        assert l50 == 1
    
    def test_empty(self):
        """Test N50 with empty list."""
        n50, l50 = compute_n50([])
        assert n50 == 0
        assert l50 == 0
    
    def test_two_contigs(self):
        """Test N50 with two contigs."""
        lengths = [300, 700]
        n50, l50 = compute_n50(lengths)
        assert n50 == 700
        assert l50 == 1


class TestComputeGCContent:
    """Tests for GC content computation."""
    
    def test_all_at(self):
        """Test GC content with all A/T."""
        assert compute_gc_content(["AAAA", "TTTT"]) == 0.0
    
    def test_all_gc(self):
        """Test GC content with all G/C."""
        assert compute_gc_content(["GGGG", "CCCC"]) == 1.0
    
    def test_mixed(self):
        """Test GC content with mixed bases."""
        # ACGT has 2 GC out of 4 = 50%
        assert compute_gc_content(["ACGT"]) == 0.5
    
    def test_with_n(self):
        """Test GC content with N's."""
        # ACGT NNNN: 2 GC out of 8 = 25%
        assert compute_gc_content(["ACGT", "NNNN"]) == 0.25
    
    def test_empty(self):
        """Test GC content with empty sequences."""
        assert compute_gc_content([]) == 0.0


class TestCountGaps:
    """Tests for gap counting."""
    
    def test_no_gaps(self):
        """Test counting gaps with no gaps."""
        assert count_gaps(["ACGTACGT"]) == 0
    
    def test_single_gap(self):
        """Test counting a single gap."""
        assert count_gaps(["ACGTNNNNACGT"]) == 1
    
    def test_multiple_gaps(self):
        """Test counting multiple gaps."""
        assert count_gaps(["ACGTNNNNACGTNNNN"]) == 2
    
    def test_gap_at_start(self):
        """Test gap at start."""
        assert count_gaps(["NNNNACGT"]) == 1
    
    def test_gap_at_end(self):
        """Test gap at end."""
        assert count_gaps(["ACGTNNNN"]) == 1


class TestComputeAssemblyStats:
    """Tests for comprehensive assembly statistics."""
    
    def test_basic_stats(self):
        """Test basic statistics computation."""
        sequences = ["ACGTACGT", "TTTTCCCC", "GGGGAAAA"]
        stats = compute_assembly_stats(sequences)
        
        assert stats.num_contigs == 3
        assert stats.total_length == 24
        assert stats.longest_contig == 8
        assert stats.shortest_contig == 8
        assert stats.gc_content == 0.5
    
    def test_empty(self):
        """Test with empty sequences."""
        stats = compute_assembly_stats([])
        assert stats.num_contigs == 0
        assert stats.total_length == 0
    
    def test_single_contig(self):
        """Test with a single contig."""
        stats = compute_assembly_stats(["ACGTACGTACGT"])
        assert stats.num_contigs == 1
        assert stats.total_length == 12
        assert stats.longest_contig == 12
        assert stats.shortest_contig == 12
    
    def test_summary(self):
        """Test summary output."""
        sequences = ["ACGTACGT", "TTTTCCCC"]
        stats = compute_assembly_stats(sequences)
        summary = stats.summary()
        
        assert "Assembly Statistics:" in summary
        assert "Number of contigs: 2" in summary


class TestAssemblyStatsRepr:
    """Tests for AssemblyStats representation."""
    
    def test_repr(self):
        """Test string representation."""
        stats = AssemblyStats(
            num_contigs=5,
            total_length=10000,
            longest_contig=5000,
            shortest_contig=1000,
            n50=5000,
            l50=2,
            gc_content=0.45,
            num_gaps=3,
        )
        repr_str = repr(stats)
        assert "contigs: 5" in repr_str
        assert "N50: 5000" in repr_str


class TestCompareAssemblies:
    """Tests for comparing assemblies to reference."""
    
    def test_perfect_assembly(self):
        """Test comparison of perfect assembly."""
        reference = "ACGTACGTACGT"
        assembled = ["ACGTACGT", "ACGT"]
        
        result = compare_assemblies(assembled, reference)
        assert result["reference_length"] == 12
        assert result["assembled_length"] == 12
    
    def test_partial_assembly(self):
        """Test comparison of partial assembly."""
        reference = "ACGTACGTACGTACGT"
        assembled = ["ACGTACGT"]
        
        result = compare_assemblies(assembled, reference)
        assert result["assembled_length"] == 8
        assert result["coverage"] == 0.5
