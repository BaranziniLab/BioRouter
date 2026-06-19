use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

use crate::graph::Graph;
use crate::path::PathResult;

/// Depth-First Search: finds *a* path from start to goal (not necessarily
/// shortest). Useful for reachability testing and cycle detection. Returns
/// `None` if the goal is unreachable.
pub fn dfs<N, G>(graph: &G, start: &N, goal: &N) -> Option<PathResult<N>>
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

    let mut visited = HashMap::new();
    visited.insert(start.clone(), None);

    let mut stack = vec![start.clone()];

    while let Some(current) = stack.pop() {
        if current == *goal {
            return Some(reconstruct_path(&visited, goal));
        }

        for (neighbor, _weight) in graph.neighbors(&current) {
            if !visited.contains_key(&neighbor) {
                visited.insert(neighbor.clone(), Some(current.clone()));
                stack.push(neighbor);
            }
        }
    }

    None
}

fn reconstruct_path<N: Eq + Hash + Clone + Debug>(
    came_from: &HashMap<N, Option<N>>,
    goal: &N,
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
        total_cost: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::AdjacencyListGraph;

    fn linear_graph() -> AdjacencyListGraph<i32> {
        let mut g = AdjacencyListGraph::new_directed();
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        g.add_edge(2, 3, 1.0);
        g
    }

    #[test]
    fn test_dfs_finds_path() {
        let g = linear_graph();
        let result = dfs(&g, &0, &3).unwrap();
        assert_eq!(result.nodes, vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_dfs_same_node() {
        let g = linear_graph();
        let result = dfs(&g, &1, &1).unwrap();
        assert_eq!(result.nodes, vec![1]);
    }

    #[test]
    fn test_dfs_unreachable() {
        let g = linear_graph();
        // 3 cannot reach 0 in a directed graph
        assert!(dfs(&g, &3, &0).is_none());
    }

    #[test]
    fn test_dfs_branching() {
        let mut g = AdjacencyListGraph::new_directed();
        g.add_edge(0, 1, 1.0);
        g.add_edge(0, 2, 1.0);
        g.add_edge(1, 3, 1.0);
        g.add_edge(2, 3, 1.0);
        let result = dfs(&g, &0, &3).unwrap();
        // DFS explores one branch first; the exact path depends on neighbor ordering
        assert_eq!(result.nodes.first(), Some(&0));
        assert_eq!(result.nodes.last(), Some(&3));
    }
}
