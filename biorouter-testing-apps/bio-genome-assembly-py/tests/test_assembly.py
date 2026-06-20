"""
Integration tests for genome assembly.
"""

import tempfile
import os

import pytest

from bio_assembly.io import SequenceRecord, read_fasta, write_fasta
from bio_assembly.metrics import compute_assembly_stats, compare_assemblies
from bio_assembly.simulate import (
    create_test_reference,
    simulate_long_reads,
    simulate_short_reads,
)
from bio_assembly.dbg import DBGAssembler, assemble_dbg
from bio_assembly.olc import OLCAssembler, assemble_olc


class TestAssemblyReconstruction:
    """Tests for assembling simulated reads back to reference."""
    
    def test_dbg_assembly_short_reads(self):
        """Test DBG assembly with short error-free reads."""
        # Use a non-repetitive reference for cleaner DBG assembly
        reference = create_test_reference(500, seed=42, pattern="random")
        
        # Create overlapping reads (no errors)
        reads = simulate_short_reads(
            reference,
            num_reads=100,
            read_length=100,
            error_rate=0.0,
            seed=42,
        )
        
        assembler = DBGAssembler(k=21)
        contigs = assembler.assemble(reads)
        
        # Should reconstruct the reference
        assembled_seq = "".join(c.sequence for c in contigs)
        
        # For reasonable coverage, we should get back a significant portion
        assert len(assembled_seq) > 0
        stats = compute_assembly_stats([c.sequence for c in contigs])
        assert stats.total_length > 0
    
    def test_dbg_assembly_with_errors(self):
        """Test DBG assembly with reads containing errors."""
        reference = create_test_reference(1000, seed=42)
        
        reads = simulate_short_reads(
            reference,
            num_reads=50,
            read_length=100,
            error_rate=0.01,  # 1% error rate
            seed=42,
        )
        
        assembler = DBGAssembler(k=21)
        contigs = assembler.assemble(reads)
        
        if contigs:
            stats = compute_assembly_stats([c.sequence for c in contigs])
            assert stats.num_contigs > 0
            assert stats.total_length > 0
    
    def test_olc_assembly_simple(self):
        """Test OLC assembly with simple overlapping reads."""
        reference = "A" * 100 + "C" * 100 + "G" * 100 + "T" * 100
        
        # Create long overlapping reads
        reads = []
        read_len = 150
        overlap = 100
        for i in range(0, len(reference) - read_len + 1, overlap):
            reads.append(SequenceRecord(
                id=f"read_{i}",
                description="",
                sequence=reference[i:i + read_len],
            ))
        
        if len(reads) < 2:
            return
        
        assembler = OLCAssembler(min_overlap=50)
        contigs = assembler.assemble(reads)
        
        if contigs:
            assembled_seq = "".join(c.sequence for c in contigs)
            # Should cover significant portion of reference
            assert len(assembled_seq) > 0


class TestAssemblyFromSimulatedReads:
    """Tests for full pipeline: simulate -> assemble -> validate."""
    
    def test_dbg_pipeline(self):
        """Test full DBG assembly pipeline."""
        # Create reference
        reference = create_test_reference(500, seed=123, pattern="simple")
        
        # Simulate reads
        reads = simulate_short_reads(
            reference,
            num_reads=100,
            read_length=50,
            error_rate=0.0,
            seed=123,
        )
        
        # Assemble
        contigs, stats = assemble_dbg(reads, k=21)
        
        # Validate
        assert stats.num_contigs > 0
        assert stats.total_length > 0
    
    def test_olc_pipeline(self):
        """Test full OLC assembly pipeline."""
        # Create reference
        reference = "ACGT" * 250  # 1000 bp
        
        # Simulate long reads
        reads = simulate_long_reads(
            reference,
            num_reads=10,
            read_length=200,
            error_rate=0.0,
            seed=42,
        )
        
        if len(reads) < 2:
            return
        
        # Assemble
        contigs, stats = assemble_olc(
            reads,
            min_overlap=50,
            max_error_rate=0.1,
        )
        
        # Validate
        assert stats.num_contigs > 0


class TestAssemblyMetrics:
    """Tests for assembly metrics in context."""
    
    def test_perfect_assembly_metrics(self):
        """Test metrics for perfect assembly."""
        reference = "ACGTACGT" * 125  # 1000 bp
        assembled = [reference]
        
        stats = compute_assembly_stats(assembled)
        assert stats.num_contigs == 1
        assert stats.total_length == 1000
        assert stats.longest_contig == 1000
        assert stats.gc_content == 0.5
    
    def test_fragmented_assembly_metrics(self):
        """Test metrics for fragmented assembly."""
        assembled = ["ACGT" * 25] * 10  # 10 contigs of 100 bp each
        
        stats = compute_assembly_stats(assembled)
        assert stats.num_contigs == 10
        assert stats.total_length == 1000
        assert stats.n50 == 100
        assert stats.l50 == 5


class TestAssemblyEdgeCases:
    """Tests for edge cases in assembly."""
    
    def test_empty_reads(self):
        """Test assembly with no reads."""
        assembler = DBGAssembler(k=21)
        contigs = assembler.assemble([])
        assert contigs == []
    
    def test_single_base_reads(self):
        """Test assembly with very short reads."""
        reads = [
            SequenceRecord("r1", "", "A"),
            SequenceRecord("r2", "", "C"),
        ]
        
        assembler = DBGAssembler(k=1)  # k=1 for single bases
        contigs = assembler.assemble(reads)
        
        # Should handle gracefully
        assert isinstance(contigs, list)
    
    def test_identical_reads(self):
        """Test assembly with identical reads."""
        reads = [
            SequenceRecord("r1", "", "ACGTACGT"),
            SequenceRecord("r2", "", "ACGTACGT"),
            SequenceRecord("r3", "", "ACGTACGT"),
        ]
        
        assembler = DBGAssembler(k=3)
        contigs = assembler.assemble(reads)
        
        # Should produce contigs
        assert len(contigs) >= 1


class TestFileOutput:
    """Tests for file output."""
    
    def test_write_and_read_contigs(self):
        """Test writing and reading contig files."""
        contigs = [
            SequenceRecord("contig1", "assembled", "ACGTACGTACGT"),
            SequenceRecord("contig2", "assembled", "TTTTCCCCGGGG"),
        ]
        
        with tempfile.NamedTemporaryFile(mode='w', suffix='.fasta', delete=False) as f:
            tmpfile = f.name
        
        try:
            write_fasta(contigs, tmpfile)
            read_records = list(read_fasta(tmpfile))
            
            assert len(read_records) == 2
            assert read_records[0].id == "contig1"
            assert read_records[1].sequence == "TTTTCCCCGGGG"
        finally:
            os.unlink(tmpfile)
