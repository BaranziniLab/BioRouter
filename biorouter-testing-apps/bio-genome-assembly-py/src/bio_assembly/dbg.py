"""
De Bruijn Graph (DBG) genome assembler.

Implements the de Bruijn graph assembly algorithm:
1. Build k-mer graph from reads
2. Simplify graph (collapse unitigs, remove tips/bubbles)
3. Emit contigs from simplified graph

Best suited for short reads (Illumina) where k-mer analysis is efficient.
"""

from __future__ import annotations

from collections import defaultdict
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Set, Tuple

from .io import SequenceRecord
from .metrics import AssemblyStats, compute_assembly_stats_from_records


@dataclass
class KmerNode:
    """Node in the de Bruijn graph representing a k-mer."""
    
    kmer: str
    count: int = 1  # Coverage depth
    in_edges: List[str] = field(default_factory=list)  # Preceding k-mers
    out_edges: List[str] = field(default_factory=list)  # Following k-mers
    
    def __hash__(self):
        return hash(self.kmer)
    
    def __eq__(self, other):
        return self.kmer == other.kmer


class DeBruijnGraph:
    """
    De Bruijn graph for genome assembly.
    
    Nodes are (k-1)-mers, edges represent k-mers.
    """
    
    def __init__(self, k: int = 21):
        """
        Initialize the de Bruijn graph.
        
        Args:
            k: K-mer size
        """
        self.k = k
        self.nodes: Dict[str, KmerNode] = {}
        self.edges: Dict[str, List[str]] = defaultdict(list)
        self.reverse_edges: Dict[str, List[str]] = defaultdict(list)
        self.kmer_counts: Dict[str, int] = defaultdict(int)
    
    def add_kmer(self, kmer: str) -> None:
        """
        Add a k-mer to the graph.
        
        Args:
            kmer: K-mer sequence string
        """
        if len(kmer) != self.k:
            raise ValueError(f"K-mer must be length {self.k}, got {len(kmer)}")
        
        kmer = kmer.upper()
        self.kmer_counts[kmer] += 1
        
        # Nodes are (k-1)-mers
        prefix = kmer[:-1]
        suffix = kmer[1:]
        
        # Add nodes
        if prefix not in self.nodes:
            self.nodes[prefix] = KmerNode(kmer=prefix)
        if suffix not in self.nodes:
            self.nodes[suffix] = KmerNode(kmer=suffix)
        
        # Add edge
        if suffix not in self.edges[prefix]:
            self.edges[prefix].append(suffix)
            self.reverse_edges[suffix].append(prefix)
    
    def build_from_reads(self, reads: List[SequenceRecord]) -> None:
        """
        Build the graph from a list of reads.
        
        Args:
            reads: List of read SequenceRecord objects
        """
        for read in reads:
            seq = read.sequence.upper()
            # Add all k-mers from this read
            for i in range(len(seq) - self.k + 1):
                kmer = seq[i:i + self.k]
                self.add_kmer(kmer)
    
    def get_node_coverage(self, node: str) -> int:
        """Get coverage depth for a node."""
        return self.nodes[node].count if node in self.nodes else 0
    
    def is_tip(self, node: str) -> bool:
        """
        Check if a node is a tip (dead end with low coverage).
        
        Args:
            node: Node k-1-mer
            
        Returns:
            True if node is a tip
        """
        if node not in self.nodes:
            return False
        
        in_count = len(self.reverse_edges[node])
        out_count = len(self.edges[node])
        
        return (in_count == 0 and out_count == 1) or (in_count == 1 and out_count == 0)
    
    def remove_tip(self, node: str, max_tip_length: int = 10) -> bool:
        """
        Remove a tip from the graph.
        
        Args:
            node: Starting node of the tip
            max_tip_length: Maximum length of tip to remove
            
        Returns:
            True if tip was removed
        """
        if not self.is_tip(node):
            return False
        
        # Trace the tip
        tip_path = [node]
        current = node
        
        if len(self.edges[node]) == 1:
            # Forward tip
            while len(self.edges[current]) == 1 and len(tip_path) < max_tip_length:
                next_node = self.edges[current][0]
                if next_node == node:  # Cycle
                    break
                tip_path.append(next_node)
                current = next_node
                if not self.is_tip(current) and len(self.edges[current]) != 0:
                    break
        else:
            # Backward tip
            while len(self.reverse_edges[current]) == 1 and len(tip_path) < max_tip_length:
                prev_node = self.reverse_edges[current][0]
                if prev_node == node:  # Cycle
                    break
                tip_path.insert(0, prev_node)
                current = prev_node
                if not self.is_tip(current) and len(self.reverse_edges[current]) != 0:
                    break
        
        # Only remove if tip is short enough
        if len(tip_path) <= max_tip_length:
            for n in tip_path:
                self._remove_node(n)
            return True
        
        return False
    
    def _remove_node(self, node: str) -> None:
        """Remove a node and its edges from the graph."""
        if node in self.nodes:
            del self.nodes[node]
        
        # Remove forward edges
        if node in self.edges:
            for next_node in self.edges[node]:
                if node in self.reverse_edges[next_node]:
                    self.reverse_edges[next_node].remove(node)
            del self.edges[node]
        
        # Remove reverse edges
        if node in self.reverse_edges:
            for prev_node in self.reverse_edges[node]:
                if node in self.edges[prev_node]:
                    self.edges[prev_node].remove(node)
            del self.reverse_edges[node]
    
    def collapse_unitig(self, start: str) -> List[str]:
        """
        Collapse a unitig (linear path) into a single contig.
        
        Args:
            start: Starting node of the unitig
            
        Returns:
            List of nodes in the unitig
        """
        unitig = [start]
        current = start
        visited = {start}
        
        # Extend forward
        while True:
            out_nodes = [n for n in self.edges[current] if n not in visited]
            if len(out_nodes) != 1:
                break
            next_node = out_nodes[0]
            unitig.append(next_node)
            visited.add(next_node)
            current = next_node
        
        return unitig
    
    def simplify(self, max_tip_length: int = 10, 
                min_coverage: float = 0.1) -> None:
        """
        Simplify the graph by removing tips and low-coverage nodes.
        
        Args:
            max_tip_length: Maximum length of tips to remove
            min_coverage: Minimum coverage fraction to keep a node
        """
        # Calculate mean coverage
        if not self.nodes:
            return
        
        coverages = [n.count for n in self.nodes.values()]
        mean_coverage = sum(coverages) / len(coverages) if coverages else 0
        threshold = mean_coverage * min_coverage
        
        # Remove low coverage nodes
        to_remove = [n for n, node in self.nodes.items() if node.count < threshold]
        for node in to_remove:
            self._remove_node(node)
        
        # Remove tips iteratively
        changed = True
        while changed:
            changed = False
            tips = [n for n in self.nodes if self.is_tip(n)]
            for tip in tips:
                if self.remove_tip(tip, max_tip_length):
                    changed = True
    
    def extract_contigs(self) -> List[str]:
        """
        Extract contigs from the simplified graph.
        
        Returns:
            List of contig sequences
        """
        contigs = []
        visited = set()
        
        for start_node in list(self.nodes.keys()):
            if start_node in visited:
                continue
            
            # Check if this is a start of a unitig (no incoming edges or junction)
            in_count = len(self.reverse_edges[start_node])
            if in_count > 1:
                continue  # Junction, skip
            
            # Collapse unitig
            unitig = self.collapse_unitig(start_node)
            
            if len(unitig) < 2:
                continue
            
            # Build sequence from unitig
            # First node contributes k-1 bases, each subsequent adds 1
            seq = unitig[0]
            for node in unitig[1:]:
                seq += node[-1]
            
            contigs.append(seq)
            visited.update(unitig)
        
        # Also add isolated nodes as single-kmer contigs
        for node in self.nodes:
            if node not in visited:
                contigs.append(node)
        
        return contigs


class DBGAssembler:
    """
    De Bruijn Graph genome assembler.
    
    Usage:
        assembler = DBGAssembler(k=21)
        contigs = assembler.assemble(reads)
    """
    
    def __init__(self, k: int = 21, 
                 min_coverage: float = 0.1,
                 max_tip_length: int = 10):
        """
        Initialize the DBG assembler.
        
        Args:
            k: K-mer size
            min_coverage: Minimum coverage fraction to keep
            max_tip_length: Maximum length of tips to remove
        """
        self.k = k
        self.min_coverage = min_coverage
        self.max_tip_length = max_tip_length
    
    def assemble(self, reads: List[SequenceRecord]) -> List[SequenceRecord]:
        """
        Assemble reads into contigs using de Bruijn graph.
        
        Args:
            reads: List of read SequenceRecord objects
            
        Returns:
            List of assembled contig SequenceRecord objects
        """
        if not reads:
            return []
        
        # Build graph
        graph = DeBruijnGraph(k=self.k)
        graph.build_from_reads(reads)
        
        # Update node coverage from kmer_counts
        for node, kmer in graph.nodes.items():
            # Coverage is average of k-mers that contain this node
            # For simplicity, use the k-mer count of the node itself
            graph.nodes[node].count = graph.kmer_counts.get(kmer, 1)
        
        # Simplify graph
        graph.simplify(
            max_tip_length=self.max_tip_length,
            min_coverage=self.min_coverage,
        )
        
        # Extract contigs
        contig_sequences = graph.extract_contigs()
        
        # Convert to SequenceRecords
        contigs = []
        for i, seq in enumerate(contig_sequences):
            contigs.append(SequenceRecord(
                id=f"contig_{i + 1}",
                description=f"k={self.k} de Bruijn assembly",
                sequence=seq,
            ))
        
        return contigs


def assemble_dbg(reads: List[SequenceRecord],
                k: int = 21,
                **kwargs) -> Tuple[List[SequenceRecord], AssemblyStats]:
    """
    Convenience function to assemble reads using DBG algorithm.
    
    Args:
        reads: List of read SequenceRecord objects
        k: K-mer size
        **kwargs: Additional arguments for DBGAssembler
        
    Returns:
        Tuple of (contigs, assembly_stats)
    """
    assembler = DBGAssembler(k=k, **kwargs)
    contigs = assembler.assemble(reads)
    stats = compute_assembly_stats_from_records(contigs)
    
    return contigs, stats
