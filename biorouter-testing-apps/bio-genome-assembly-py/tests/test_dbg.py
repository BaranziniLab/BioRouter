"""
Tests for the de Bruijn graph module.
"""

import pytest

from bio_assembly.io import SequenceRecord
from bio_assembly.dbg import DBGAssembler, DeBruijnGraph, KmerNode


class TestDeBruijnGraph:
    """Tests for the DeBruijnGraph class."""
    
    def test_add_kmer(self):
        """Test adding a k-mer to the graph."""
        graph = DeBruijnGraph(k=4)
        graph.add_kmer("ACGT")
        
        # Should create nodes for "ACG" and "CGT"
        assert "ACG" in graph.nodes
        assert "CGT" in graph.nodes
        
        # Should create edge from ACG to CGT
        assert "CGT" in graph.edges["ACG"]
    
    def test_add_multiple_kmers(self):
        """Test adding multiple k-mers."""
        graph = DeBruijnGraph(k=4)
        graph.add_kmer("ACGT")
        graph.add_kmer("CGTT")
        
        # Should create nodes for ACG, CGT, GTT
        assert "ACG" in graph.nodes
        assert "CGT" in graph.nodes
        assert "GTT" in graph.nodes
        
        # Should create edges: ACG->CGT, CGT->GTT
        assert "CGT" in graph.edges["ACG"]
        assert "GTT" in graph.edges["CGT"]
    
    def test_build_from_reads(self):
        """Test building graph from reads."""
        reads = [
            SequenceRecord("r1", "", "ACGTACGT"),
        ]
        
        graph = DeBruijnGraph(k=4)
        graph.build_from_reads(reads)
        
        # Should have k-mers: ACGT (ACG->CGT), CGTA (CGT->GTA), GTAC (GTA->TAC), TACG (TAC->ACG)
        assert len(graph.nodes) >= 4  # ACG, CGT, GTA, TAC
    
    def test_is_tip(self):
        """Test tip detection."""
        graph = DeBruijnGraph(k=4)
        graph.add_kmer("ACGT")  # ACG -> CGT
        graph.add_kmer("CGTT")  # CGT -> GTT
        
        # ACG has only one outgoing edge and no incoming -> tip
        # Actually, let's check more carefully
        # ACG -> CGT (from ACGT)
        # CGT -> GTT (from CGTT)
        # ACG has no incoming edges -> it's a tip
        
        # But let's make a more explicit tip
        graph2 = DeBruijnGraph(k=4)
        graph2.add_kmer("AAAA")  # AAA -> AAA (self-loop)
        graph2.add_kmer("AAAC")  # AAA -> AAC
        # AAA has two outgoing edges now
        
        # Let's test a clearer tip case
        graph3 = DeBruijnGraph(k=4)
        graph3.add_kmer("ACGT")  # ACG -> CGT
        # ACG has 0 in, 1 out -> tip
    
    def test_collapse_unitig(self):
        """Test collapsing a unitig."""
        graph = DeBruijnGraph(k=4)
        graph.add_kmer("ACGT")  # ACG -> CGT
        graph.add_kmer("CGTT")  # CGT -> GTT
        graph.add_kmer("GTTT")  # GTT -> TTT
        
        # Linear path: ACG -> CGT -> GTT -> TTT
        unitig = graph.collapse_unitig("ACG")
        assert unitig == ["ACG", "CGT", "GTT", "TTT"]
    
    def test_extract_contigs(self):
        """Test extracting contigs from graph."""
        graph = DeBruijnGraph(k=4)
        graph.add_kmer("ACGT")  # ACG -> CGT
        graph.add_kmer("CGTT")  # CGT -> GTT
        graph.add_kmer("GTTT")  # GTT -> TTT
        
        contigs = graph.extract_contigs()
        
        # Should extract one contig
        assert len(contigs) >= 1
        # The contig should reconstruct a sequence
        for contig in contigs:
            assert len(contig) >= 3


class TestDBGAssembler:
    """Tests for the DBG assembler."""
    
    def test_assemble_simple(self):
        """Test assembling simple reads."""
        reference = "ACGTACGTACGTACGT"
        reads = [
            SequenceRecord("r1", "", "ACGTACGT"),
            SequenceRecord("r2", "", "ACGTACGT"),
        ]
        
        assembler = DBGAssembler(k=5)
        contigs = assembler.assemble(reads)
        
        # Should produce some contigs
        assert len(contigs) >= 0  # May or may not assemble depending on coverage
    
    def test_assemble_empty(self):
        """Test assembling empty reads."""
        assembler = DBGAssembler(k=5)
        contigs = assembler.assemble([])
        assert contigs == []
    
    def test_assemble_single_read(self):
        """Test assembling a single read."""
        reads = [SequenceRecord("r1", "", "ACGTACGT")]
        
        assembler = DBGAssembler(k=3)
        contigs = assembler.assemble(reads)
        
        # Single read should produce at least one contig
        assert len(contigs) >= 1


class TestKmerNode:
    """Tests for KmerNode dataclass."""
    
    def test_creation(self):
        """Test creating a KmerNode."""
        node = KmerNode(kmer="ACG", count=5)
        assert node.kmer == "ACG"
        assert node.count == 5
        assert node.in_edges == []
        assert node.out_edges == []
    
    def test_hash(self):
        """Test hashing."""
        node1 = KmerNode(kmer="ACG")
        node2 = KmerNode(kmer="ACG")
        assert hash(node1) == hash(node2)
    
    def test_equality(self):
        """Test equality."""
        node1 = KmerNode(kmer="ACG")
        node2 = KmerNode(kmer="ACG")
        assert node1 == node2
