//! Generic directed/undirected weighted graph with adjacency-list storage.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// A weighted edge from `src` to `dst` with a given `weight`.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub src: usize,
    pub dst: usize,
    pub weight: f64,
}

/// Adjacency-list representation of a weighted graph.
///
/// Vertices are `usize` IDs (0-based recommended). Supports directed and
/// undirected graphs. Duplicate edges are silently overwritten (last wins).
#[derive(Debug, Clone)]
pub struct Graph {
    adj: BTreeMap<usize, Vec<(usize, f64)>>,
    pub directed: bool,
    edge_count: usize,
}

impl Graph {
    /// Create a new empty graph. `directed = true` for digraph, `false` for undirected.
    pub fn new(directed: bool) -> Self {
        Graph {
            adj: BTreeMap::new(),
            directed,
            edge_count: 0,
        }
    }

    /// Ensure a vertex exists (no-op if already present).
    pub fn add_vertex(&mut self, v: usize) {
        self.adj.entry(v).or_default();
    }

    /// Add a weighted edge. For undirected graphs, the reverse edge is added automatically.
    pub fn add_edge(&mut self, src: usize, dst: usize, weight: f64) {
        self.add_vertex(src);
        self.add_vertex(dst);
        // Avoid duplicate edges: remove existing edge to same dst first
        let neighbours = self.adj.get_mut(&src).unwrap();
        if let Some(pos) = neighbours.iter().position(|&(d, _)| d == dst) {
            neighbours[pos] = (dst, weight);
        } else {
            neighbours.push((dst, weight));
            self.edge_count += 1;
        }
        if !self.directed && src != dst {
            let neighbours = self.adj.get_mut(&dst).unwrap();
            if let Some(pos) = neighbours.iter().position(|&(d, _)| d == src) {
                neighbours[pos] = (src, weight);
            } else {
                neighbours.push((src, weight));
            }
        }
    }

    /// Number of vertices.
    pub fn vertex_count(&self) -> usize {
        self.adj.len()
    }

    /// Number of edges (for undirected: each pair counted once).
    pub fn edge_count(&self) -> usize {
        self.edge_count
    }

    /// Iterator over vertex IDs.
    pub fn vertices(&self) -> impl Iterator<Item = usize> + '_ {
        self.adj.keys().copied()
    }

    /// Neighbours of a vertex: `&[(dst, weight)]`.
    pub fn neighbours(&self, v: usize) -> &[(usize, f64)] {
        self.adj.get(&v).map_or(&[], |n| n.as_slice())
    }

    /// All edges as `(src, dst, weight)`. For undirected graphs, each edge appears once.
    pub fn edges(&self) -> Vec<Edge> {
        let mut edges = Vec::new();
        let mut seen = BTreeSet::new();
        for (&src, neighbours) in &self.adj {
            for &(dst, weight) in neighbours {
                let key = if self.directed || src <= dst {
                    (src, dst)
                } else {
                    (dst, src)
                };
                if seen.insert(key) {
                    edges.push(Edge { src, dst, weight });
                }
            }
        }
        edges
    }

    /// Reverse graph (only meaningful for directed graphs).
    pub fn reverse(&self) -> Self {
        let mut rev = Graph::new(self.directed);
        for (&src, neighbours) in &self.adj {
            rev.add_vertex(src);
            for &(dst, weight) in neighbours {
                rev.add_vertex(dst);
                let neighbours = rev.adj.get_mut(&dst).unwrap();
                if let Some(pos) = neighbours.iter().position(|&(d, _)| d == src) {
                    neighbours[pos] = (src, weight);
                } else {
                    neighbours.push((src, weight));
                }
            }
        }
        rev.edge_count = self.edge_count;
        rev
    }
}

impl fmt::Display for Graph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Graph({}, directed={}, vertices={}, edges={})",
            if self.directed { "digraph" } else { "undirected" },
            self.directed,
            self.vertex_count(),
            self.edge_count()
        )?;
        for (&v, neighbours) in &self.adj {
            for &(dst, w) in neighbours {
                let arrow = if self.directed { "->" } else { "--" };
                writeln!(f, "  {v} {arrow} {dst}  (w={w})")?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_undirected_graph() {
        let mut g = Graph::new(false);
        g.add_edge(0, 1, 5.0);
        g.add_edge(1, 2, 3.0);
        assert_eq!(g.vertex_count(), 3);
        assert_eq!(g.edge_count(), 2);
        // Undirected: 0→1 and 1→0
        assert_eq!(g.neighbours(0).len(), 1);
        assert_eq!(g.neighbours(1).len(), 2);
    }

    #[test]
    fn test_directed_graph() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 2.0);
        g.add_edge(1, 0, 3.0);
        assert_eq!(g.vertex_count(), 2);
        assert_eq!(g.edge_count(), 2);
        assert_eq!(g.neighbours(0).len(), 1);
        assert_eq!(g.neighbours(1).len(), 1);
    }

    #[test]
    fn test_reverse_graph() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 2.0);
        let rev = g.reverse();
        assert_eq!(rev.neighbours(2).len(), 1);
        assert_eq!(rev.neighbours(2)[0].0, 1);
    }

    #[test]
    fn test_empty_graph() {
        let g = Graph::new(false);
        assert_eq!(g.vertex_count(), 0);
        assert_eq!(g.edge_count(), 0);
        assert!(g.edges().is_empty());
    }

    #[test]
    fn test_overwrite_edge() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_edge(0, 1, 9.0);
        assert_eq!(g.edge_count(), 1);
        assert_eq!(g.neighbours(0)[0].1, 9.0);
    }
}
