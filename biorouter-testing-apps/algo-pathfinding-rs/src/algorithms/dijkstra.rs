use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

use crate::graph::Graph;
use crate::path::PathResult;

/// Dijkstra's algorithm: finds the shortest weighted path from start to goal.
/// Requires all edge weights to be non-negative. Returns `None` if the goal
/// is unreachable.
pub fn dijkstra<N, G>(graph: &G, start: &N, goal: &N) -> Option<PathResult<N>>
where
    N: Eq + Hash + Clone + Debug,
    G: Graph<N>,
{
    if start == goal {
        return Some(PathResult {
            nodes: vec![start.clone()],
            total_cost: 0.0,
        });
    }

    let mut dist: HashMap<N, f64> = HashMap::new();
    let mut came_from: HashMap<N, Option<N>> = HashMap::new();
    let mut visited = std::collections::HashSet::new();

    dist.insert(start.clone(), 0.0);
    came_from.insert(start.clone(), None);

    // Simple priority queue via a sorted Vec (sufficient for educational purpose).
    let mut pq: Vec<(f64, N)> = vec![(0.0, start.clone())];

    while let Some((cost, current)) = pop_min(&mut pq) {
        if visited.contains(&current) {
            continue;
        }
        visited.insert(current.clone());

        if current == *goal {
            return Some(reconstruct(&came_from, goal, cost));
        }

        for (neighbor, weight) in graph.neighbors(&current) {
            if visited.contains(&neighbor) {
                continue;
            }
            let new_cost = cost + weight;
            let better = dist
                .get(&neighbor)
                .is_none_or(|&old| new_cost < old);
            if better {
                dist.insert(neighbor.clone(), new_cost);
                came_from.insert(neighbor.clone(), Some(current.clone()));
                pq.push((new_cost, neighbor));
            }
        }
    }

    None
}

fn pop_min<N: Clone>(pq: &mut Vec<(f64, N)>) -> Option<(f64, N)> {
    if pq.is_empty() {
        return None;
    }
    let mut min_idx = 0;
    for i in 1..pq.len() {
        if pq[i].0 < pq[min_idx].0 {
            min_idx = i;
        }
    }
    Some(pq.swap_remove(min_idx))
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
    fn test_dijkstra_simple() {
        let mut g = AdjacencyListGraph::new_undirected();
        g.add_edge(0, 1, 4.0);
        g.add_edge(0, 2, 1.0);
        g.add_edge(2, 1, 2.0);
        g.add_edge(1, 3, 1.0);

        let result = dijkstra(&g, &0, &3).unwrap();
        assert_eq!(result.nodes, vec![0, 2, 1, 3]);
        assert!((result.total_cost - 4.0).abs() < 1e-9);
    }

    #[test]
    fn test_dijkstra_same_node() {
        let g: AdjacencyListGraph<i32> = AdjacencyListGraph::new_undirected();
        let result = dijkstra(&g, &5, &5).unwrap();
        assert_eq!(result.total_cost, 0.0);
    }

    #[test]
    fn test_dijkstra_unreachable() {
        let mut g = AdjacencyListGraph::new_directed();
        g.add_node(0);
        g.add_node(1);
        assert!(dijkstra(&g, &0, &1).is_none());
    }

    #[test]
    fn test_dijkstra_multiple_paths() {
        let mut g = AdjacencyListGraph::new_undirected();
        // Path A: 0->1->2 = cost 10
        g.add_edge(0, 1, 5.0);
        g.add_edge(1, 2, 5.0);
        // Path B: 0->3->4->2 = cost 7
        g.add_edge(0, 3, 1.0);
        g.add_edge(3, 4, 2.0);
        g.add_edge(4, 2, 4.0);

        let result = dijkstra(&g, &0, &2).unwrap();
        assert!((result.total_cost - 7.0).abs() < 1e-9);
        assert_eq!(result.nodes, vec![0, 3, 4, 2]);
    }

    #[test]
    fn test_dijkstra_grid() {
        // 3x3 grid
        let mut g = AdjacencyListGraph::new_undirected();
        for r in 0..3 {
            for c in 0..3 {
                let id = r * 3 + c;
                if c + 1 < 3 {
                    g.add_edge(id, r * 3 + c + 1, 1.0);
                }
                if r + 1 < 3 {
                    g.add_edge(id, (r + 1) * 3 + c, 1.0);
                }
            }
        }
        let result = dijkstra(&g, &0, &8).unwrap();
        assert!((result.total_cost - 4.0).abs() < 1e-9);
    }
}
