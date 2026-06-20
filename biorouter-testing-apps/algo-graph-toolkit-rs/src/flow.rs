//! Max-flow: Edmonds-Karp (BFS-based Ford-Fulkerson).

use std::collections::VecDeque;

use crate::graph::Graph;

/// Edmonds-Karp maximum flow algorithm.
///
/// Works on directed graphs where edge weights represent capacities.
/// Returns `(max_flow_value, residual_graph)`.
///
/// For undirected graphs, each undirected edge is treated as two directed edges
/// of the same capacity.
pub fn edmonds_karp(graph: &Graph, source: usize, sink: usize) -> (f64, Vec<Vec<f64>>) {
    let vertices: Vec<usize> = graph.vertices().collect();
    if vertices.is_empty() {
        return (0.0, Vec::new());
    }
    let max_v = *vertices.iter().max().unwrap() + 1;

    // Build capacity matrix
    let mut cap = vec![vec![0.0f64; max_v]; max_v];
    for edge in graph.edges() {
        cap[edge.src][edge.dst] += edge.weight;
        if !graph.directed {
            cap[edge.dst][edge.src] += edge.weight;
        }
    }

    let mut flow = 0.0;
    let mut residual = cap.clone();

    loop {
        // BFS to find augmenting path
        let mut parent: Vec<Option<usize>> = vec![None; max_v];
        let mut visited = vec![false; max_v];
        let mut queue = VecDeque::new();
        visited[source] = true;
        queue.push_back(source);

        while let Some(u) = queue.pop_front() {
            for v in 0..max_v {
                if !visited[v] && residual[u][v] > 1e-12 {
                    visited[v] = true;
                    parent[v] = Some(u);
                    if v == sink {
                        break;
                    }
                    queue.push_back(v);
                }
            }
            if visited[sink] {
                break;
            }
        }

        if !visited[sink] {
            break; // No augmenting path
        }

        // Find bottleneck
        let mut path_flow = f64::INFINITY;
        let mut v = sink;
        while let Some(u) = parent[v] {
            path_flow = path_flow.min(residual[u][v]);
            v = u;
        }

        // Update residual capacities
        v = sink;
        while let Some(u) = parent[v] {
            residual[u][v] -= path_flow;
            residual[v][u] += path_flow;
            v = u;
        }

        flow += path_flow;
    }

    (flow, residual)
}

/// Reconstruct the flow on each original edge from the residual graph.
pub fn extract_flow(graph: &Graph, residual: &[Vec<f64>]) -> Vec<(usize, usize, f64)> {
    let mut flows = Vec::new();
    for edge in graph.edges() {
        let original_cap = edge.weight;
        let remaining = residual[edge.src][edge.dst];
        let used = original_cap - remaining;
        if used > 1e-12 {
            flows.push((edge.src, edge.dst, used));
        }
    }
    flows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classic_flow_graph() -> Graph {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 16.0);
        g.add_edge(0, 2, 13.0);
        g.add_edge(1, 2, 4.0);
        g.add_edge(1, 3, 12.0);
        g.add_edge(2, 1, 10.0);
        g.add_edge(2, 4, 14.0);
        g.add_edge(3, 2, 9.0);
        g.add_edge(3, 5, 20.0);
        g.add_edge(4, 3, 7.0);
        g.add_edge(4, 5, 4.0);
        g
    }

    #[test]
    fn test_edmonds_karp_classic() {
        let g = classic_flow_graph();
        let (flow, _) = edmonds_karp(&g, 0, 5);
        assert!((flow - 23.0).abs() < 1e-9);
    }

    #[test]
    fn test_edmonds_karp_simple() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 10.0);
        g.add_edge(0, 2, 10.0);
        g.add_edge(1, 3, 4.0);
        g.add_edge(2, 3, 8.0);
        g.add_edge(1, 2, 2.0);
        let (flow, _) = edmonds_karp(&g, 0, 3);
        assert!((flow - 12.0).abs() < 1e-9);
    }

    #[test]
    fn test_edmonds_karp_no_path() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 10.0);
        g.add_vertex(5);
        let (flow, _) = edmonds_karp(&g, 0, 5);
        assert!((flow - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_edmonds_karp_single_edge() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 5.0);
        let (flow, _) = edmonds_karp(&g, 0, 1);
        assert!((flow - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_edmonds_karp_empty() {
        let g = Graph::new(true);
        let (flow, _) = edmonds_karp(&g, 0, 1);
        assert!((flow - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_extract_flow() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 10.0);
        g.add_edge(1, 2, 5.0);
        let (_, residual) = edmonds_karp(&g, 0, 2);
        let flows = extract_flow(&g, &residual);
        assert!(!flows.is_empty());
    }

    #[test]
    fn test_parallel_edges() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 3.0);
        g.add_edge(0, 1, 5.0); // Overwrites
        let (flow, _) = edmonds_karp(&g, 0, 1);
        assert!((flow - 5.0).abs() < 1e-9);
    }
}
