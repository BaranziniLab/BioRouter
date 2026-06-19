use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

use crate::graph::Graph;
use crate::path::PathResult;

/// A* algorithm: informed search that uses a heuristic function to guide the
/// search toward the goal. The heuristic must be admissible (never overestimate)
/// for optimality. Returns `None` if the goal is unreachable.
pub fn astar<N, G, H>(graph: &G, start: &N, goal: &N, heuristic: H) -> Option<PathResult<N>>
where
    N: Eq + Hash + Clone + Debug,
    G: Graph<N>,
    H: Fn(&N) -> f64,
{
    if start == goal {
        return Some(PathResult {
            nodes: vec![start.clone()],
            total_cost: 0.0,
        });
    }

    let mut g_score: HashMap<N, f64> = HashMap::new();
    let mut f_score: HashMap<N, f64> = HashMap::new();
    let mut came_from: HashMap<N, Option<N>> = HashMap::new();
    let mut visited = std::collections::HashSet::new();

    g_score.insert(start.clone(), 0.0);
    f_score.insert(start.clone(), heuristic(start));
    came_from.insert(start.clone(), None);

    // Open set as (f_score, node)
    let mut open: Vec<(f64, N)> = vec![(heuristic(start), start.clone())];

    while let Some((_, current)) = pop_min_f(&mut open) {
        if visited.contains(&current) {
            continue;
        }
        visited.insert(current.clone());

        if current == *goal {
            let cost = *g_score.get(&current).unwrap();
            return Some(reconstruct(&came_from, goal, cost));
        }

        let current_g = *g_score.get(&current).unwrap_or(&f64::INFINITY);

        for (neighbor, weight) in graph.neighbors(&current) {
            if visited.contains(&neighbor) {
                continue;
            }
            let tentative_g = current_g + weight;
            let better = g_score
                .get(&neighbor)
                .is_none_or(|&old| tentative_g < old);
            if better {
                g_score.insert(neighbor.clone(), tentative_g);
                let f = tentative_g + heuristic(&neighbor);
                f_score.insert(neighbor.clone(), f);
                came_from.insert(neighbor.clone(), Some(current.clone()));
                open.push((f, neighbor));
            }
        }
    }

    None
}

fn pop_min_f<N: Clone>(open: &mut Vec<(f64, N)>) -> Option<(f64, N)> {
    if open.is_empty() {
        return None;
    }
    let mut min_idx = 0;
    for i in 1..open.len() {
        if open[i].0 < open[min_idx].0 {
            min_idx = i;
        }
    }
    Some(open.swap_remove(min_idx))
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
    use crate::heuristics;

    /// Build a weighted grid graph and return `(graph, goal)` for A* tests.
    fn grid_graph() -> (AdjacencyListGraph<(i32, i32)>, (i32, i32)) {
        let mut g = AdjacencyListGraph::new_undirected();
        let rows = 5;
        let cols = 5;
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
        (g, (4, 4))
    }

    #[test]
    fn test_astar_grid_manhattan() {
        let (g, goal) = grid_graph();
        let h = |n: &(i32, i32)| heuristics::manhattan(n, &goal);
        let result = astar(&g, &(0, 0), &goal, h).unwrap();
        // Manhattan distance from (0,0) to (4,4) = 8
        assert!((result.total_cost - 8.0).abs() < 1e-9);
        assert_eq!(result.nodes.first(), Some(&(0, 0)));
        assert_eq!(result.nodes.last(), Some(&(4, 4)));
    }

    #[test]
    fn test_astar_with_zero_heuristic_is_dijkstra() {
        let mut g = AdjacencyListGraph::new_undirected();
        g.add_edge(0, 1, 2.0);
        g.add_edge(0, 2, 5.0);
        g.add_edge(1, 2, 1.0);

        let result_d = crate::algorithms::dijkstra(&g, &0, &2).unwrap();
        let result_a = astar(&g, &0, &2, |_: &i32| 0.0).unwrap();
        assert!((result_d.total_cost - result_a.total_cost).abs() < 1e-9);
        assert_eq!(result_d.nodes, result_a.nodes);
    }

    #[test]
    fn test_astar_directed() {
        let mut g = AdjacencyListGraph::new_directed();
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        g.add_edge(0, 2, 5.0);

        let result = astar(&g, &0, &2, |_: &i32| 0.0).unwrap();
        assert!((result.total_cost - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_astar_unreachable() {
        let mut g = AdjacencyListGraph::new_directed();
        g.add_node(0);
        g.add_node(1);
        assert!(astar(&g, &0, &1, |_: &i32| 0.0).is_none());
    }
}
