use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

/// Core trait for graph data structures.
pub trait Graph<N: Eq + Hash + Clone + Debug> {
    /// Return all nodes in the graph.
    fn nodes(&self) -> Vec<N>;

    /// Return all neighbors of a node (outgoing edges for directed graphs).
    fn neighbors(&self, node: &N) -> Vec<(N, f64)>;

    /// Check if a node exists in the graph.
    fn contains_node(&self, node: &N) -> bool;

    /// Number of nodes.
    fn node_count(&self) -> usize;

    /// Number of edges.
    fn edge_count(&self) -> usize;
}

/// Directed or undirected weighted graph backed by an adjacency list.
#[derive(Debug, Clone)]
pub struct AdjacencyListGraph<N: Eq + Hash + Clone + Debug> {
    /// Adjacency list: node -> list of (neighbor, weight).
    adjacency: HashMap<N, Vec<(N, f64)>>,
    /// Whether edges are bidirectional.
    directed: bool,
    edge_count: usize,
}

impl<N: Eq + Hash + Clone + Debug> AdjacencyListGraph<N> {
    /// Create a new directed graph.
    pub fn new_directed() -> Self {
        Self {
            adjacency: HashMap::new(),
            directed: true,
            edge_count: 0,
        }
    }

    /// Create a new undirected graph.
    pub fn new_undirected() -> Self {
        Self {
            adjacency: HashMap::new(),
            directed: false,
            edge_count: 0,
        }
    }

    /// Add a node to the graph. Returns true if the node was newly inserted.
    pub fn add_node(&mut self, node: N) -> bool {
        if self.adjacency.contains_key(&node) {
            return false;
        }
        self.adjacency.insert(node, Vec::new());
        true
    }

    /// Add a weighted edge. Nodes are created automatically if missing.
    pub fn add_edge(&mut self, from: N, to: N, weight: f64) {
        // Ensure both endpoint nodes exist in the adjacency map.
        self.adjacency
            .entry(from.clone())
            .or_default()
            .push((to.clone(), weight));
        // For directed graphs, create the `to` entry if absent (empty neighbor list).
        self.adjacency.entry(to.clone()).or_default();
        if !self.directed {
            self.adjacency
                .entry(to)
                .or_default()
                .push((from, weight));
        }
        self.edge_count += 1;
    }

    /// Whether this graph treats edges as directed.
    pub fn is_directed(&self) -> bool {
        self.directed
    }

    /// Return the weight of an edge, if it exists.
    pub fn edge_weight(&self, from: &N, to: &N) -> Option<f64> {
        self.adjacency
            .get(from)?
            .iter()
            .find(|(n, _)| n == to)
            .map(|(_, w)| *w)
    }
}

impl<N: Eq + Hash + Clone + Debug> Graph<N> for AdjacencyListGraph<N> {
    fn nodes(&self) -> Vec<N> {
        self.adjacency.keys().cloned().collect()
    }

    fn neighbors(&self, node: &N) -> Vec<(N, f64)> {
        self.adjacency.get(node).cloned().unwrap_or_default()
    }

    fn contains_node(&self, node: &N) -> bool {
        self.adjacency.contains_key(node)
    }

    fn node_count(&self) -> usize {
        self.adjacency.len()
    }

    fn edge_count(&self) -> usize {
        self.edge_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_nodes_and_edges() {
        let mut g = AdjacencyListGraph::new_undirected();
        g.add_node(1);
        g.add_node(2);
        g.add_edge(1, 2, 3.5);
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 1);
    }

    #[test]
    fn test_undirected_neighbors() {
        let mut g = AdjacencyListGraph::new_undirected();
        g.add_edge('a', 'b', 1.0);
        let nbs = g.neighbors(&'a');
        assert_eq!(nbs.len(), 1);
        assert_eq!(nbs[0].0, 'b');
        let nbs_b = g.neighbors(&'b');
        assert_eq!(nbs_b.len(), 1);
        assert_eq!(nbs_b[0].0, 'a');
    }

    #[test]
    fn test_directed_neighbors() {
        let mut g = AdjacencyListGraph::new_directed();
        g.add_edge("x", "y", 2.0);
        assert_eq!(g.neighbors(&"x").len(), 1);
        // y has no outgoing edges in a directed graph
        assert_eq!(g.neighbors(&"y").len(), 0);
    }

    #[test]
    fn test_auto_create_nodes() {
        let mut g = AdjacencyListGraph::new_undirected();
        g.add_edge(10, 20, 1.0);
        assert_eq!(g.node_count(), 2);
        assert!(g.contains_node(&10));
        assert!(g.contains_node(&20));
    }

    #[test]
    fn test_edge_weight() {
        let mut g = AdjacencyListGraph::new_directed();
        g.add_edge(0, 1, 5.0);
        assert_eq!(g.edge_weight(&0, &1), Some(5.0));
        assert_eq!(g.edge_weight(&1, &0), None);
    }
}
