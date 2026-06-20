use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

use crate::graph::Graph;
use crate::path::PathResult;

/// Bellman-Ford algorithm: computes shortest paths from a single source,
/// tolerating negative edge weights. Returns an error if a negative-weight
/// cycle is reachable from the source.
///
/// # Returns
/// - `Ok(Some(path))` — shortest path to goal
/// - `Ok(None)` — goal unreachable
/// - `Err(())` — negative cycle detected
#[allow(clippy::result_unit_err)]
pub fn bellman_ford<N, G>(
    graph: &G,
    start: &N,
    goal: &N,
) -> Result<Option<PathResult<N>>, ()>
where
    N: Eq + Hash + Clone + Debug,
    G: Graph<N>,
{
    if start == goal {
        return Ok(Some(PathResult {
            nodes: vec![start.clone()],
            total_cost: 0.0,
        }));
    }

    let nodes = graph.nodes();
    if nodes.is_empty() {
        return Ok(None);
    }

    let mut dist: HashMap<N, f64> = HashMap::new();
    let mut came_from: HashMap<N, Option<N>> = HashMap::new();

    dist.insert(start.clone(), 0.0);
    came_from.insert(start.clone(), None);

    // Build edge list from adjacency info
    let mut edges: Vec<(N, N, f64)> = Vec::new();
    for node in &nodes {
        for (neighbor, weight) in graph.neighbors(node) {
            edges.push((node.clone(), neighbor, weight));
        }
    }

    let n = nodes.len();

    // Relax edges V-1 times
    for _ in 0..n.saturating_sub(1) {
        let mut updated = false;
        for (u, v, w) in &edges {
            let du = match dist.get(u) {
                Some(&d) => d,
                None => continue,
            };
            let new_cost = du + w;
            let better = dist.get(v).is_none_or(|&old| new_cost < old);
            if better {
                dist.insert(v.clone(), new_cost);
                came_from.insert(v.clone(), Some(u.clone()));
                updated = true;
            }
        }
        if !updated {
            break; // Early termination
        }
    }

    // Check for negative cycles
    for (u, v, w) in &edges {
        if let Some(&du) = dist.get(u) {
            if du + w < *dist.get(v).unwrap_or(&f64::INFINITY) {
                return Err(());
            }
        }
    }

    match dist.get(goal) {
        Some(&cost) => Ok(Some(reconstruct(&came_from, goal, cost))),
        None => Ok(None),
    }
}

fn reconstruct<N: Eq + Hash + Clone + Debug>(
    came_from: &HashMap<N, Option<N>>,
    goal: &N,
    total_cost: f64,
) -> PathResult<N> {
    let mut path = Vec::new();
    let mut current = goal.clone();
    path.push(current.clone());

    while let Some(Some(parent)) = came_from.get(&current) {
        path.push(parent.clone());
        current = parent.clone();
    }
    path.reverse();

    PathResult {
        nodes: path,
        total_cost,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::AdjacencyListGraph;

    #[test]
    fn test_bf_simple() {
        // Must be directed — an undirected negative edge creates a spurious
        // negative cycle via the reverse traversal (1↔2 both get weight -3).
        let mut g = AdjacencyListGraph::new_directed();
        g.add_edge(0, 1, 4.0);
        g.add_edge(0, 2, 5.0);
        g.add_edge(1, 2, -3.0);

        let result = bellman_ford(&g, &0, &2).unwrap().unwrap();
        // 0 -> 1 -> 2 = 4 + (-3) = 1
        assert!((result.total_cost - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_bf_negative_cycle() {
        let mut g = AdjacencyListGraph::new_directed();
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, -1.0);
        g.add_edge(2, 0, -1.0); // cycle: 0+1-1-1 = -1 per loop

        let result = bellman_ford(&g, &0, &2);
        assert!(result.is_err());
    }

    #[test]
    fn test_bf_no_negative_cycle_positive_graph() {
        let mut g = AdjacencyListGraph::new_directed();
        g.add_edge(0, 1, 2.0);
        g.add_edge(1, 2, 3.0);
        g.add_edge(0, 2, 10.0);

        let result = bellman_ford(&g, &0, &2).unwrap().unwrap();
        assert!((result.total_cost - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_bf_unreachable() {
        let mut g = AdjacencyListGraph::new_directed();
        g.add_node(0);
        g.add_node(1);
        assert!(bellman_ford(&g, &0, &1).unwrap().is_none());
    }

    #[test]
    fn test_bf_same_node() {
        let mut g = AdjacencyListGraph::new_directed();
        g.add_node(0);
        let result = bellman_ford(&g, &0, &0).unwrap().unwrap();
        assert!((result.total_cost).abs() < 1e-9);
    }
}
