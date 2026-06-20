"""
Overlap-Layout-Consensus (OLC) genome assembler.

Implements the classical OLC assembly algorithm:
1. Compute all pairwise overlaps between reads
2. Build overlap graph
3. Find assembly layout (greedy path finding)
4. Generate consensus sequences for each contig

Best suited for long reads (PacBio, Nanopore) where overlaps are informative.
"""

from __future__ import annotations

from collections import defaultdict
from typing import Dict, List, Optional, Set, Tuple

from .consensus import consensus_from_paths, merge_two_reads
from .io import SequenceRecord
from .metrics import AssemblyStats, compute_assembly_stats_from_records
from .overlap import Overlap, build_overlap_graph, find_overlaps, transitive_reduction


class OLCAssembler:
    """
    Overlap-Layout-Consensus assembler.
    
    Usage:
        assembler = OLCAssembler(min_overlap=500)
        contigs = assembler.assemble(reads)
    """
    
    def __init__(self,
                 min_overlap: int = 500,
                 max_error_rate: float = 0.1,
                 max_errors: Optional[int] = None,
                 both_strands: bool = True,
                 perform_transitive_reduction: bool = True,
                 max_reads: Optional[int] = None):
        """
        Initialize the OLC assembler.
        
        Args:
            min_overlap: Minimum overlap length to consider
            max_error_rate: Maximum error rate in overlaps
            max_errors: Maximum absolute errors (overrides error_rate)
            both_strands: Check both strands for overlaps
            perform_transitive_reduction: Remove transitive edges
            max_reads: Limit number of reads (for memory)
        """
        self.min_overlap = min_overlap
        self.max_error_rate = max_error_rate
        self.max_errors = max_errors
        self.both_strands = both_strands
        self.perform_transitive_reduction = perform_transitive_reduction
        self.max_reads = max_reads
    
    def assemble(self, reads: List[SequenceRecord]) -> List[SequenceRecord]:
        """
        Assemble reads into contigs using OLC algorithm.
        
        Args:
            reads: List of read SequenceRecord objects
            
        Returns:
            List of assembled contig SequenceRecord objects
        """
        if not reads:
            return []
        
        if len(reads) == 1:
            return [reads[0]]
        
        # Step 1: Compute overlaps
        overlaps = find_overlaps(
            reads,
            min_overlap=self.min_overlap,
            max_error_rate=self.max_error_rate,
            max_errors=self.max_errors,
            both_strands=self.both_strands,
            max_reads=self.max_reads,
        )
        
        # Step 2: Build overlap graph
        graph = build_overlap_graph(reads, overlaps)
        
        # Step 3: Transitive reduction (optional)
        if self.perform_transitive_reduction:
            reduced_overlaps = transitive_reduction(overlaps)
            graph = build_overlap_graph(reads, reduced_overlaps)
        
        # Step 4: Find paths through the graph
        paths = self._find_assembly_paths(graph, len(reads))
        
        # Step 5: Generate consensus for each path
        contigs = consensus_from_paths(reads, paths, graph)
        
        return contigs
    
    def _find_assembly_paths(self, graph: Dict[int, List[Overlap]], 
                            num_reads: int) -> List[List[int]]:
        """
        Find assembly paths through the overlap graph using greedy algorithm.
        
        Args:
            graph: Overlap graph (adjacency list)
            num_reads: Total number of reads
            
        Returns:
            List of paths (each path is list of read indices)
        """
        visited = set()
        paths = []
        
        # Sort reads by number of overlaps (start with most connected)
        read_scores = []
        for i in range(num_reads):
            out_degree = len(graph.get(i, []))
            # Count in-degree
            in_degree = sum(1 for ovs in graph.values() for ov in ovs if ov.read_b == i)
            score = out_degree + in_degree
            read_scores.append((score, i))
        
        read_scores.sort(reverse=True)
        
        for _, start_read in read_scores:
            if start_read in visited:
                continue
            
            # Build path greedily from this start
            path = [start_read]
            visited.add(start_read)
            
            # Extend forward
            current = start_read
            while True:
                best_next = self._find_best_next(current, graph, visited)
                if best_next is None:
                    break
                path.append(best_next)
                visited.add(best_next)
                current = best_next
            
            # Extend backward from start
            current = start_read
            while True:
                best_prev = self._find_best_prev(current, graph, visited)
                if best_prev is None:
                    break
                path.insert(0, best_prev)
                visited.add(best_prev)
                current = best_prev
            
            paths.append(path)
        
        return paths
    
    def _find_best_next(self, read_idx: int, 
                       graph: Dict[int, List[Overlap]], 
                       visited: Set[int]) -> Optional[int]:
        """Find the best next read in a path."""
        candidates = graph.get(read_idx, [])
        
        # Filter out visited reads
        candidates = [ov for ov in candidates if ov.read_b not in visited]
        
        if not candidates:
            return None
        
        # Sort by overlap score (prefer higher similarity) and length
        candidates.sort(key=lambda ov: (-ov.score, -ov.length))
        
        return candidates[0].read_b
    
    def _find_best_prev(self, read_idx: int,
                       graph: Dict[int, List[Overlap]],
                       visited: Set[int]) -> Optional[int]:
        """Find the best previous read in a path."""
        # Look for reads that have overlap TO this read
        candidates = []
        for source, ovs in graph.items():
            for ov in ovs:
                if ov.read_b == read_idx and source not in visited:
                    candidates.append(ov)
        
        if not candidates:
            return None
        
        # Sort by overlap score
        candidates.sort(key=lambda ov: (-ov.score, -ov.length))
        
        return candidates[0].read_a


def assemble_olc(reads: List[SequenceRecord],
                min_overlap: int = 500,
                max_error_rate: float = 0.1,
                **kwargs) -> Tuple[List[SequenceRecord], AssemblyStats]:
    """
    Convenience function to assemble reads using OLC algorithm.
    
    Args:
        reads: List of read SequenceRecord objects
        min_overlap: Minimum overlap length
        max_error_rate: Maximum error rate
        **kwargs: Additional arguments for OLCAssembler
        
    Returns:
        Tuple of (contigs, assembly_stats)
    """
    assembler = OLCAssembler(
        min_overlap=min_overlap,
        max_error_rate=max_error_rate,
        **kwargs,
    )
    
    contigs = assembler.assemble(reads)
    stats = compute_assembly_stats_from_records(contigs)
    
    return contigs, stats
