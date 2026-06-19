//! Topological sort (DFS-based).

use crate::graph::Graph;

/// Topological sort of a DAG. Returns `Some(order)` or `None` if the graph
/// contains a cycle.
///
/// Vertices are returned in topological order (edges go from earlier to later).
pub fn topological_sort(graph: &Graph) -> Option<Vec<usize>> {
    if !graph.directed {
        return None; // topological sort requires directed graph
    }

    let n = graph.vertex_count();
    // Collect all vertices first
    let vertices: Vec<usize> = graph.vertices().collect();
    if vertices.is_empty() {
        return Some(Vec::new());
    }

    let max_v = *vertices.iter().max().unwrap();
    let mut order = Vec::with_capacity(n);

    // Iterative DFS-based topological sort
    // 0 = not started, 1 = visiting, 2 = done
    let mut state = vec![0u8; max_v + 1];
    let mut stack: Vec<(usize, usize)> = Vec::new(); // (vertex, neighbour index)

    for &start in &vertices {
        if state[start] != 0 {
            continue;
        }
        stack.push((start, 0));
        state[start] = 1;

        while let Some((v, ni)) = stack.pop() {
            if ni == 0 && state[v] == 2 {
                continue;
            }
            let neighbours: Vec<usize> = graph
                .neighbours(v)
                .iter()
                .map(|&(d, _)| d)
                .collect();

            if ni < neighbours.len() {
                // Push current vertex back with incremented neighbour index
                stack.push((v, ni + 1));
                let next = neighbours[ni];
                if state[next] == 1 {
                    // Cycle detected
                    return None;
                }
                if state[next] == 0 {
                    state[next] = 1;
                    stack.push((next, 0));
                }
            } else {
                // All neighbours processed
                state[v] = 2;
                order.push(v);
            }
        }
    }

    order.reverse();
    Some(order)
}

/// Kahn's algorithm for topological sort (BFS-based).
/// Returns `Some(order)` or `None` if the graph contains a cycle.
pub fn topological_sort_kahn(graph: &Graph) -> Option<Vec<usize>> {
    if !graph.directed {
        return None;
    }

    let vertices: Vec<usize> = graph.vertices().collect();
    if vertices.is_empty() {
        return Some(Vec::new());
    }
    let max_v = *vertices.iter().max().unwrap();

    // Compute in-degrees
    let mut in_degree = vec![0usize; max_v + 1];
    for v in &vertices {
        for &(dst, _) in graph.neighbours(*v) {
            in_degree[dst] += 1;
        }
    }

    // Start with zero in-degree vertices
    let mut queue: std::collections::VecDeque<usize> = vertices
        .iter()
        .filter(|&&v| in_degree[v] == 0)
        .copied()
        .collect();

    let mut order = Vec::new();
    while let Some(v) = queue.pop_front() {
        order.push(v);
        for &(dst, _) in graph.neighbours(v) {
            in_degree[dst] -= 1;
            if in_degree[dst] == 0 {
                queue.push_back(dst);
            }
        }
    }

    if order.len() == vertices.len() {
        Some(order)
    } else {
        None // cycle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diamond_dag() -> Graph {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_edge(0, 2, 1.0);
        g.add_edge(1, 3, 1.0);
        g.add_edge(2, 3, 1.0);
        g
    }

    #[test]
    fn test_toposort_simple() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        let order = topological_sort(&g).unwrap();
        assert_eq!(order, vec![0, 1, 2]);
    }

    #[test]
    fn test_toposort_diamond() {
        let g = diamond_dag();
        let order = topological_sort(&g).unwrap();
        assert_eq!(order.len(), 4);
        let pos: Vec<usize> = order
            .iter()
            .enumerate()
            .map(|(i, _)| i)
            .collect();
        // 0 must come before 1 and 2; 1 and 2 before 3
        let i0 = order.iter().position(|&x| x == 0).unwrap();
        let i1 = order.iter().position(|&x| x == 1).unwrap();
        let i2 = order.iter().position(|&x| x == 2).unwrap();
        let i3 = order.iter().position(|&x| x == 3).unwrap();
        assert!(i0 < i1);
        assert!(i0 < i2);
        assert!(i1 < i3);
        assert!(i2 < i3);
    }

    #[test]
    fn test_toposort_cycle() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        g.add_edge(2, 0, 1.0);
        assert!(topological_sort(&g).is_none());
    }

    #[test]
    fn test_toposort_undirected() {
        let mut g = Graph::new(false);
        g.add_edge(0, 1, 1.0);
        assert!(topological_sort(&g).is_none());
    }

    #[test]
    fn test_toposort_empty() {
        let g = Graph::new(true);
        let order = topological_sort(&g).unwrap();
        assert!(order.is_empty());
    }

    #[test]
    fn test_kahn_simple() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        let order = topological_sort_kahn(&g).unwrap();
        assert_eq!(order, vec![0, 1, 2]);
    }

    #[test]
    fn test_kahn_diamond() {
        let g = diamond_dag();
        let order = topological_sort_kahn(&g).unwrap();
        assert_eq!(order.len(), 4);
    }

    #[test]
    fn test_kahn_cycle() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 0, 1.0);
        assert!(topological_sort_kahn(&g).is_none());
    }
}
