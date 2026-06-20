use std::collections::{HashMap, VecDeque};
use std::fmt::Debug;
use std::hash::Hash;

use crate::graph::Graph;
use crate::path::PathResult;

/// Breadth-First Search: finds the shortest path in terms of number of hops
/// (edge count), ignoring weights. Returns `None` if the goal is unreachable.
pub fn bfs<N, G>(graph: &G, start: &N, goal: &N) -> Option<PathResult<N>>
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

    let mut queue = VecDeque::new();
    let mut visited = HashMap::new(); // node -> parent

    queue.push_back(start.clone());
    visited.insert(start.clone(), None);

    while let Some(current) = queue.pop_front() {
        for (neighbor, _weight) in graph.neighbors(&current) {
            if visited.contains_key(&neighbor) {
                continue;
            }
            visited.insert(neighbor.clone(), Some(current.clone()));

            if neighbor == *goal {
                return Some(reconstruct_path(visited, goal));
            }
            queue.push_back(neighbor);
        }
    }

    None
}

fn reconstruct_path<N: Eq + Hash + Clone + Debug>(
    came_from: HashMap<N, Option<N>>,
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

    fn sample_graph() -> AdjacencyListGraph<i32> {
        let mut g = AdjacencyListGraph::new_undirected();
        g.add_edge(0, 1, 1.0);
        g.add_edge(0, 2, 1.0);
        g.add_edge(1, 3, 1.0);
        g.add_edge(2, 3, 1.0);
        g.add_edge(3, 4, 1.0);
        g
    }

    #[test]
    fn test_bfs_finds_shortest_hop_path() {
        let g = sample_graph();
        let result = bfs(&g, &0, &4).unwrap();
        assert_eq!(result.nodes, vec![0, 1, 3, 4]);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_bfs_same_start_goal() {
        let g = sample_graph();
        let result = bfs(&g, &0, &0).unwrap();
        assert_eq!(result.nodes, vec![0]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_bfs_unreachable() {
        let mut g = AdjacencyListGraph::new_undirected();
        g.add_node(0);
        g.add_node(99);
        assert!(bfs(&g, &0, &99).is_none());
    }

    #[test]
    fn test_bfs_direct_path() {
        let mut g = AdjacencyListGraph::new_undirected();
        g.add_edge(0, 1, 5.0);
        g.add_edge(1, 2, 5.0);
        let result = bfs(&g, &0, &2).unwrap();
        assert_eq!(result.nodes, vec![0, 1, 2]);
    }
}
