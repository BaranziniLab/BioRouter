//! Shortest paths: Dijkstra, Bellman-Ford, Floyd-Warshall.

use std::collections::{BinaryHeap, HashMap};
use std::cmp::Reverse;

use crate::graph::Graph;

const INF: f64 = f64::INFINITY;

/// Dijkstra's algorithm (non-negative weights only).
///
/// Returns `(distances, predecessors)`. `distances[v]` is the shortest distance
/// from `source` to `v`, or `INF` if unreachable. `predecessors[v]` is the
/// previous vertex on the shortest path (or `None` for source / unreachable).
pub fn dijkstra(graph: &Graph, source: usize) -> (Vec<f64>, Vec<Option<usize>>) {
    let vertices: Vec<usize> = graph.vertices().collect();
    if vertices.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let max_v = *vertices.iter().max().unwrap();
    let mut dist = vec![INF; max_v + 1];
    let mut prev: Vec<Option<usize>> = vec![None; max_v + 1];
    let mut visited = vec![false; max_v + 1];

    dist[source] = 0.0;
    // Min-heap: (distance_as_int, vertex)
    // We use integer encoding for f64 ordering
    let mut heap: BinaryHeap<Reverse<(i64, usize)>> = BinaryHeap::new();
    heap.push(Reverse((0, source)));

    while let Some(Reverse((_, u))) = heap.pop() {
        if visited[u] {
            continue;
        }
        visited[u] = true;

        for &(v, w) in graph.neighbours(u) {
            let alt = dist[u] + w;
            if alt < dist[v] {
                dist[v] = alt;
                prev[v] = Some(u);
                heap.push(Reverse(((alt * 1000.0) as i64, v)));
            }
        }
    }
    (dist, prev)
}

/// Bellman-Ford algorithm (handles negative weights).
///
/// Returns `Ok((distances, predecessors))` or `Err(())` if a negative-weight
/// cycle is reachable from `source`.
pub fn bellman_ford(
    graph: &Graph,
    source: usize,
) -> Result<(Vec<f64>, Vec<Option<usize>>), ()> {
    let vertices: Vec<usize> = graph.vertices().collect();
    if vertices.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let max_v = *vertices.iter().max().unwrap();
    let edges = graph.edges();

    let mut dist = vec![INF; max_v + 1];
    let mut prev: Vec<Option<usize>> = vec![None; max_v + 1];
    dist[source] = 0.0;

    // Relax edges |V|-1 times
    for _ in 0..vertices.len() - 1 {
        for edge in &edges {
            if dist[edge.src] < INF {
                let alt = dist[edge.src] + edge.weight;
                if alt < dist[edge.dst] {
                    dist[edge.dst] = alt;
                    prev[edge.dst] = Some(edge.src);
                }
                // For undirected: also relax reverse direction
                if !graph.directed {
                    if dist[edge.dst] < INF {
                        let alt = dist[edge.dst] + edge.weight;
                        if alt < dist[edge.src] {
                            dist[edge.src] = alt;
                            prev[edge.src] = Some(edge.dst);
                        }
                    }
                }
            }
        }
    }

    // Check for negative cycles
    for edge in &edges {
        if dist[edge.src] < INF {
            if dist[edge.src] + edge.weight < dist[edge.dst] {
                return Err(());
            }
            if !graph.directed && dist[edge.dst] < INF {
                if dist[edge.dst] + edge.weight < dist[edge.src] {
                    return Err(());
                }
            }
        }
    }

    Ok((dist, prev))
}

/// Floyd-Warshall all-pairs shortest paths.
///
/// Returns `dist[i][j]` = shortest distance from vertex `i` to vertex `j`,
/// indexed by the *original* vertex IDs. Disconnected pairs are `INF`.
///
/// Internally the O(V³) computation runs on a compact `n×n` matrix (where
/// `n` = number of vertices) via an id→index map, so non-contiguous vertex
/// IDs (e.g. {0, 1, 5}) are handled without wasting memory.
pub fn floyd_warshall(graph: &Graph) -> Vec<Vec<f64>> {
    let vertices: Vec<usize> = graph.vertices().collect();
    let n = vertices.len();
    if n == 0 {
        return Vec::new();
    }

    let max_v = *vertices.iter().max().unwrap();
    let out_size = max_v + 1;

    // Compact id→index map for the n×n computation
    let id_to_idx: HashMap<usize, usize> =
        vertices.iter().enumerate().map(|(i, &v)| (v, i)).collect();

    let mut dist = vec![vec![INF; n]; n];

    // Diagonal = 0
    for i in 0..n {
        dist[i][i] = 0.0;
    }

    // Edge weights
    for edge in graph.edges() {
        let i = id_to_idx[&edge.src];
        let j = id_to_idx[&edge.dst];
        dist[i][j] = dist[i][j].min(edge.weight);
        if !graph.directed {
            dist[j][i] = dist[j][i].min(edge.weight);
        }
    }

    // Relaxation
    for k in 0..n {
        for i in 0..n {
            for j in 0..n {
                if dist[i][k] < INF && dist[k][j] < INF {
                    let alt = dist[i][k] + dist[k][j];
                    if alt < dist[i][j] {
                        dist[i][j] = alt;
                    }
                }
            }
        }
    }

    // Expand back to original-vertex-ID indexed matrix
    let mut result = vec![vec![INF; out_size]; out_size];
    for &v in &vertices {
        result[v][v] = 0.0;
    }
    for &u in &vertices {
        for &v in &vertices {
            let i = id_to_idx[&u];
            let j = id_to_idx[&v];
            result[u][v] = dist[i][j];
        }
    }
    result
}

/// Reconstruct shortest path from predecessors.
pub fn reconstruct_path(prev: &[Option<usize>], source: usize, target: usize) -> Option<Vec<usize>> {
    if prev[target].is_none() && source != target {
        return None;
    }
    let mut path = Vec::new();
    let mut current = target;
    loop {
        path.push(current);
        if current == source {
            break;
        }
        match prev[current] {
            Some(p) => current = p,
            None => return None,
        }
    }
    path.reverse();
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_graph() -> Graph {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 10.0);
        g.add_edge(0, 2, 3.0);
        g.add_edge(1, 2, 1.0);
        g.add_edge(1, 3, 2.0);
        g.add_edge(2, 1, 4.0);
        g.add_edge(2, 3, 8.0);
        g.add_edge(2, 4, 2.0);
        g.add_edge(3, 4, 7.0);
        g.add_edge(4, 3, 9.0);
        g
    }

    #[test]
    fn test_dijkstra() {
        let g = sample_graph();
        let (dist, prev) = dijkstra(&g, 0);
        assert!((dist[0] - 0.0).abs() < 1e-9);
        assert!((dist[1] - 7.0).abs() < 1e-9);
        assert!((dist[2] - 3.0).abs() < 1e-9);
        assert!((dist[3] - 9.0).abs() < 1e-9);
        assert!((dist[4] - 5.0).abs() < 1e-9);

        let path = reconstruct_path(&prev, 0, 3).unwrap();
        assert_eq!(path, vec![0, 2, 1, 3]);
    }

    #[test]
    fn test_dijkstra_unreachable() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_vertex(5);
        let (dist, _) = dijkstra(&g, 0);
        assert_eq!(dist[5], INF);
    }

    #[test]
    fn test_dijkstra_single_vertex() {
        let mut g = Graph::new(true);
        g.add_vertex(0);
        let (dist, _) = dijkstra(&g, 0);
        assert!((dist[0] - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_bellman_ford() {
        let g = sample_graph();
        let (dist, _) = bellman_ford(&g, 0).unwrap();
        assert!((dist[0] - 0.0).abs() < 1e-9);
        assert!((dist[1] - 7.0).abs() < 1e-9);
        assert!((dist[2] - 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_bellman_ford_negative_cycle() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, -3.0);
        g.add_edge(2, 0, 1.0);
        assert!(bellman_ford(&g, 0).is_err());
    }

    #[test]
    fn test_bellman_ford_negative_edges_no_cycle() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 5.0);
        g.add_edge(0, 2, 8.0);
        g.add_edge(1, 2, -3.0);
        let (dist, _) = bellman_ford(&g, 0).unwrap();
        assert!((dist[0] - 0.0).abs() < 1e-9);
        assert!((dist[1] - 5.0).abs() < 1e-9);
        assert!((dist[2] - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_bellman_ford_empty() {
        let g = Graph::new(true);
        let result = bellman_ford(&g, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_floyd_warshall() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 3.0);
        g.add_edge(0, 2, 8.0);
        g.add_edge(1, 2, 2.0);
        g.add_edge(2, 0, 5.0);
        let dist = floyd_warshall(&g);
        assert!((dist[0][1] - 3.0).abs() < 1e-9);
        assert!((dist[0][2] - 5.0).abs() < 1e-9);
        assert!((dist[2][1] - 8.0).abs() < 1e-9);
    }

    #[test]
    fn test_floyd_warshall_disconnected() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_vertex(5);
        let dist = floyd_warshall(&g);
        assert_eq!(dist[0][5], INF);
    }

    #[test]
    fn test_floyd_warshall_empty() {
        let g = Graph::new(true);
        let dist = floyd_warshall(&g);
        assert!(dist.is_empty());
    }

    #[test]
    fn test_reconstruct_path_none() {
        let prev = vec![None, None];
        assert!(reconstruct_path(&prev, 0, 1).is_none());
    }
}
