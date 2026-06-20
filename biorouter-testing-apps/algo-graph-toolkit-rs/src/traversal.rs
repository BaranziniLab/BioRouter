//! BFS and DFS traversals.

use std::collections::{HashSet, VecDeque};

use crate::graph::Graph;

/// Breadth-first search starting from `source`.
/// Returns vertices in BFS order.
pub fn bfs(graph: &Graph, source: usize) -> Vec<usize> {
    let mut visited = HashSet::new();
    let mut order = Vec::new();
    let mut queue = VecDeque::new();

    visited.insert(source);
    queue.push_back(source);

    while let Some(v) = queue.pop_front() {
        order.push(v);
        for &(dst, _) in graph.neighbours(v) {
            if visited.insert(dst) {
                queue.push_back(dst);
            }
        }
    }
    order
}

/// Breadth-first search from `source`, returning the visited set and
/// parent map (for path reconstruction).  Parent of `source` is `None`.
pub fn bfs_parents(graph: &Graph, source: usize) -> (HashSet<usize>, Vec<Option<usize>>) {
    let n = graph.vertex_count();
    let mut visited = HashSet::new();
    let mut parent: Vec<Option<usize>> = vec![None; n];
    let mut queue = VecDeque::new();

    visited.insert(source);
    queue.push_back(source);

    while let Some(v) = queue.pop_front() {
        for &(dst, _) in graph.neighbours(v) {
            if visited.insert(dst) {
                parent[dst] = Some(v);
                queue.push_back(dst);
            }
        }
    }
    (visited, parent)
}

/// Depth-first search (iterative, stack-based) from `source`.
/// Returns vertices in DFS discovery order.
pub fn dfs(graph: &Graph, source: usize) -> Vec<usize> {
    let mut visited = HashSet::new();
    let mut order = Vec::new();
    let mut stack = Vec::new();

    stack.push(source);

    while let Some(v) = stack.pop() {
        if visited.insert(v) {
            order.push(v);
            // Push neighbours in reverse so that the first neighbour is processed first
            for &(dst, _) in graph.neighbours(v).iter().rev() {
                if !visited.contains(&dst) {
                    stack.push(dst);
                }
            }
        }
    }
    order
}

/// Recursive DFS with explicit finish times (for Kosaraju, etc.).
/// Returns `(discovery_order, finish_order)`.
pub fn dfs_finish_times(graph: &Graph) -> (Vec<usize>, Vec<usize>) {
    let mut visited = HashSet::new();
    let mut discovery = Vec::new();
    let mut finish_stack: Vec<(usize, bool)> = Vec::new();
    let mut finish = Vec::new();

    for v in graph.vertices() {
        if visited.contains(&v) {
            continue;
        }
        finish_stack.push((v, false));
        while let Some((node, processed)) = finish_stack.pop() {
            if processed {
                finish.push(node);
                continue;
            }
            if visited.insert(node) {
                discovery.push(node);
                finish_stack.push((node, true)); // mark for finish
                for &(dst, _) in graph.neighbours(node).iter().rev() {
                    if !visited.contains(&dst) {
                        finish_stack.push((dst, false));
                    }
                }
            }
        }
    }
    (discovery, finish)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bfs_simple() {
        // 0→1→2
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        let order = bfs(&g, 0);
        assert_eq!(order, vec![0, 1, 2]);
    }

    #[test]
    fn test_bfs_disconnected() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_vertex(5);
        let order = bfs(&g, 0);
        assert_eq!(order.len(), 2); // only 0 and 1
    }

    #[test]
    fn test_dfs_simple() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_edge(0, 2, 1.0);
        g.add_edge(1, 3, 1.0);
        let order = dfs(&g, 0);
        assert!(order.contains(&0));
        assert!(order.contains(&1));
        assert!(order.contains(&2));
        assert!(order.contains(&3));
        assert_eq!(order[0], 0);
    }

    #[test]
    fn test_dfs_finish_times() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        let (disc, fin) = dfs_finish_times(&g);
        assert_eq!(disc, vec![0, 1, 2]);
        assert_eq!(fin, vec![2, 1, 0]);
    }

    #[test]
    fn test_bfs_parents() {
        let mut g = Graph::new(false);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        let (visited, parent) = bfs_parents(&g, 0);
        assert!(visited.contains(&2));
        assert_eq!(parent[0], None);
        assert_eq!(parent[1], Some(0));
        assert_eq!(parent[2], Some(1));
    }
}
