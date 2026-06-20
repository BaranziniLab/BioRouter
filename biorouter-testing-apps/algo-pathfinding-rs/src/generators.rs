/// Graph generator functions for common graph topologies.
use crate::graph::AdjacencyListGraph;

/// Create an `rows × cols` grid graph (4-connected: up/down/left/right).
/// Node ids are `(row, col)` tuples. All edge weights are 1.0.
pub fn grid_4connected(rows: usize, cols: usize) -> AdjacencyListGraph<(usize, usize)> {
    let mut g = AdjacencyListGraph::new_undirected();
    for r in 0..rows {
        for c in 0..cols {
            if c + 1 < cols {
                g.add_edge((r, c), (r, c + 1), 1.0);
            }
            if r + 1 < rows {
                g.add_edge((r, c), (r + 1, c), 1.0);
            }
        }
    }
    g
}

/// Create an `rows × cols` grid graph (8-connected: includes diagonals).
/// Diagonal edges have weight √2. Cardinal edges have weight 1.0.
pub fn grid_8connected(rows: usize, cols: usize) -> AdjacencyListGraph<(usize, usize)> {
    let sqrt2 = std::f64::consts::SQRT_2;
    let mut g = AdjacencyListGraph::new_undirected();
    for r in 0..rows {
        for c in 0..cols {
            // Right
            if c + 1 < cols {
                g.add_edge((r, c), (r, c + 1), 1.0);
            }
            // Down
            if r + 1 < rows {
                g.add_edge((r, c), (r + 1, c), 1.0);
            }
            // Down-right
            if r + 1 < rows && c + 1 < cols {
                g.add_edge((r, c), (r + 1, c + 1), sqrt2);
            }
            // Down-left
            if r + 1 < rows && c > 0 {
                g.add_edge((r, c), (r + 1, c - 1), sqrt2);
            }
        }
    }
    g
}

/// Create a complete directed graph on `n` nodes (0..n-1) with random-looking
/// deterministic weights derived from node pairs.
pub fn complete_graph(n: usize) -> AdjacencyListGraph<usize> {
    let mut g = AdjacencyListGraph::new_directed();
    for i in 0..n {
        for j in 0..n {
            if i != j {
                let w = ((i + 1) * (j + 1)) as f64 % 13.0 + 1.0;
                g.add_edge(i, j, w);
            }
        }
    }
    g
}

/// Create a simple linear chain: `0 -> 1 -> 2 -> ... -> n-1` with given weight.
pub fn chain(n: usize, weight: f64) -> AdjacencyListGraph<usize> {
    let mut g = AdjacencyListGraph::new_directed();
    for i in 0..n.saturating_sub(1) {
        g.add_edge(i, i + 1, weight);
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Graph;

    #[test]
    fn test_grid_4connected_size() {
        let g = grid_4connected(5, 5);
        assert_eq!(g.node_count(), 25);
        // 4-connected 5x5: 4*5 horizontal + 4*5 vertical = 40 edges
        assert_eq!(g.edge_count(), 40);
    }

    #[test]
    fn test_grid_8connected_size() {
        let g = grid_8connected(3, 3);
        assert_eq!(g.node_count(), 9);
        // 3x3 grid: 12 cardinal + 4 down-right + 4 down-left = 20 edges
        assert_eq!(g.edge_count(), 20);
    }

    #[test]
    fn test_complete_graph() {
        let g = complete_graph(4);
        assert_eq!(g.node_count(), 4);
        assert_eq!(g.edge_count(), 12); // 4*3 directed edges
    }

    #[test]
    fn test_chain() {
        let g = chain(5, 2.0);
        assert_eq!(g.node_count(), 5);
        assert_eq!(g.edge_count(), 4);
    }
}
