//! Connectivity algorithms: cycle detection, bipartite check, articulation points, bridges.

use std::collections::{HashSet, VecDeque};

use crate::graph::Graph;

/// Detect whether the graph contains a cycle.
///
/// For directed graphs, uses DFS-based detection (white/grey/black).
/// For undirected graphs, uses parent-aware DFS.
pub fn has_cycle(graph: &Graph) -> bool {
    if graph.directed {
        has_cycle_directed(graph)
    } else {
        has_cycle_undirected(graph)
    }
}

fn has_cycle_directed(graph: &Graph) -> bool {
    let vertices: Vec<usize> = graph.vertices().collect();
    if vertices.is_empty() {
        return false;
    }
    let max_v = *vertices.iter().max().unwrap() + 1;
    // 0 = white (unvisited), 1 = grey (in stack), 2 = black (done)
    let mut color = vec![0u8; max_v];

    for &start in &vertices {
        if color[start] != 0 {
            continue;
        }
        // Iterative DFS
        let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
        color[start] = 1;

        while let Some((v, ni)) = stack.pop() {
            let neighbours: Vec<usize> = graph
                .neighbours(v)
                .iter()
                .map(|&(d, _)| d)
                .collect();

            if ni < neighbours.len() {
                stack.push((v, ni + 1));
                let w = neighbours[ni];
                if color[w] == 1 {
                    return true; // back edge → cycle
                }
                if color[w] == 0 {
                    color[w] = 1;
                    stack.push((w, 0));
                }
            } else {
                color[v] = 2;
            }
        }
    }
    false
}

fn has_cycle_undirected(graph: &Graph) -> bool {
    let vertices: Vec<usize> = graph.vertices().collect();
    if vertices.is_empty() {
        return false;
    }
    let max_v = *vertices.iter().max().unwrap() + 1;
    let mut visited = vec![false; max_v];

    for &start in &vertices {
        if visited[start] {
            continue;
        }
        // BFS with parent tracking
        let mut queue: VecDeque<(usize, usize)> = VecDeque::new(); // (node, parent)
        visited[start] = true;
        // Use usize::MAX as sentinel for "no parent"
        queue.push_back((start, usize::MAX));

        while let Some((v, parent)) = queue.pop_front() {
            for &(dst, _) in graph.neighbours(v) {
                if !visited[dst] {
                    visited[dst] = true;
                    queue.push_back((dst, v));
                } else if dst != parent {
                    return true; // visited neighbour that's not the parent
                }
            }
        }
    }
    false
}

/// Check if the graph is bipartite (2-colorable).
///
/// Returns `Some((set_a, set_b))` if bipartite, `None` otherwise.
pub fn is_bipartite(graph: &Graph) -> Option<(Vec<usize>, Vec<usize>)> {
    let vertices: Vec<usize> = graph.vertices().collect();
    if vertices.is_empty() {
        return Some((Vec::new(), Vec::new()));
    }
    let max_v = *vertices.iter().max().unwrap() + 1;
    let mut color = vec![i32::MAX; max_v]; // MAX = uncolored, 0/1 = colors

    let mut set_a = Vec::new();
    let mut set_b = Vec::new();

    for &start in &vertices {
        if color[start] != i32::MAX {
            continue;
        }
        color[start] = 0;
        set_a.push(start);
        let mut queue = VecDeque::new();
        queue.push_back(start);

        while let Some(v) = queue.pop_front() {
            for &(dst, _) in graph.neighbours(v) {
                if color[dst] == i32::MAX {
                    color[dst] = 1 - color[v];
                    if color[dst] == 0 {
                        set_a.push(dst);
                    } else {
                        set_b.push(dst);
                    }
                    queue.push_back(dst);
                } else if color[dst] == color[v] {
                    return None; // same colour on both ends
                }
            }
        }
    }
    Some((set_a, set_b))
}

/// Find articulation points (cut vertices) using Tarjan's algorithm.
///
/// Returns a set of vertex IDs whose removal disconnects the graph.
pub fn articulation_points(graph: &Graph) -> HashSet<usize> {
    let vertices: Vec<usize> = graph.vertices().collect();
    if vertices.is_empty() {
        return HashSet::new();
    }
    let max_v = *vertices.iter().max().unwrap() + 1;
    let mut disc = vec![0usize; max_v];
    let mut low = vec![0usize; max_v];
    let mut visited = vec![false; max_v];
    let mut ap = HashSet::new();
    let mut time = 0usize;

    for &start in &vertices {
        if visited[start] {
            continue;
        }
        // Iterative DFS for articulation points
        let mut child_count = vec![0usize; max_v];
        let mut stack: Vec<(usize, usize, usize)> = vec![(start, usize::MAX, 0)]; // (v, parent, neighbour_index)
        visited[start] = true;
        time += 1;
        disc[start] = time;
        low[start] = time;
        let root = start;

        while let Some((v, parent, ni)) = stack.pop() {
            let neighbours: Vec<usize> = graph
                .neighbours(v)
                .iter()
                .map(|&(d, _)| d)
                .collect();

            if ni < neighbours.len() {
                stack.push((v, parent, ni + 1));
                let w = neighbours[ni];
                if !visited[w] {
                    visited[w] = true;
                    time += 1;
                    disc[w] = time;
                    low[w] = time;
                    if parent == root {
                        child_count[root] += 1;
                    }
                    stack.push((w, v, 0));
                } else if w != parent {
                    low[v] = low[v].min(disc[w]);
                }
            } else {
                // Finished processing v: update parent's low
                if parent != usize::MAX {
                    low[parent] = low[parent].min(low[v]);
                    // Articulation point check (non-root)
                    if parent != root && low[v] >= disc[parent] {
                        ap.insert(parent);
                    }
                }
            }
        }
        // Root is AP if it has more than 1 child
        if child_count[root] > 1 {
            ap.insert(root);
        }
    }
    ap
}

/// Find bridges (cut edges) in the graph.
///
/// Returns a set of `(src, dst)` pairs (with `src < dst` for undirected).
pub fn bridges(graph: &Graph) -> Vec<(usize, usize)> {
    let vertices: Vec<usize> = graph.vertices().collect();
    if vertices.is_empty() {
        return Vec::new();
    }
    let max_v = *vertices.iter().max().unwrap() + 1;
    let mut disc = vec![0usize; max_v];
    let mut low = vec![0usize; max_v];
    let mut visited = vec![false; max_v];
    let mut bridge_list = Vec::new();
    let mut time = 0usize;

    for &start in &vertices {
        if visited[start] {
            continue;
        }
        let mut stack: Vec<(usize, usize, usize)> = vec![(start, usize::MAX, 0)];
        visited[start] = true;
        time += 1;
        disc[start] = time;
        low[start] = time;

        while let Some((v, parent, ni)) = stack.pop() {
            let neighbours: Vec<usize> = graph
                .neighbours(v)
                .iter()
                .map(|&(d, _)| d)
                .collect();

            if ni < neighbours.len() {
                stack.push((v, parent, ni + 1));
                let w = neighbours[ni];
                if !visited[w] {
                    visited[w] = true;
                    time += 1;
                    disc[w] = time;
                    low[w] = time;
                    stack.push((w, v, 0));
                } else if w != parent {
                    low[v] = low[v].min(disc[w]);
                }
            } else {
                if parent != usize::MAX {
                    low[parent] = low[parent].min(low[v]);
                    if low[v] > disc[parent] {
                        let edge = if parent < v {
                            (parent, v)
                        } else {
                            (v, parent)
                        };
                        bridge_list.push(edge);
                    }
                }
            }
        }
    }
    bridge_list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_cycle_directed_yes() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        g.add_edge(2, 0, 1.0);
        assert!(has_cycle(&g));
    }

    #[test]
    fn test_has_cycle_directed_no() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        assert!(!has_cycle(&g));
    }

    #[test]
    fn test_has_cycle_undirected_yes() {
        let mut g = Graph::new(false);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        g.add_edge(2, 0, 1.0);
        assert!(has_cycle(&g));
    }

    #[test]
    fn test_has_cycle_undirected_no() {
        let mut g = Graph::new(false);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        assert!(!has_cycle(&g));
    }

    #[test]
    fn test_has_cycle_empty() {
        let g = Graph::new(true);
        assert!(!has_cycle(&g));
    }

    #[test]
    fn test_is_bipartite_yes() {
        let mut g = Graph::new(false);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        g.add_edge(2, 3, 1.0);
        g.add_edge(3, 0, 1.0);
        let result = is_bipartite(&g);
        assert!(result.is_some());
    }

    #[test]
    fn test_is_bipartite_no() {
        let mut g = Graph::new(false);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        g.add_edge(2, 0, 1.0); // triangle
        assert!(is_bipartite(&g).is_none());
    }

    #[test]
    fn test_is_bipartite_single() {
        let mut g = Graph::new(false);
        g.add_vertex(0);
        assert!(is_bipartite(&g).is_some());
    }

    #[test]
    fn test_articulation_points() {
        // 0--1--2--3, with 1--4
        // Removing vertex 1 disconnects 0 from {2,3,4}
        let mut g = Graph::new(false);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        g.add_edge(2, 3, 1.0);
        g.add_edge(1, 4, 1.0);
        let ap = articulation_points(&g);
        assert!(ap.contains(&1));
        assert!(ap.contains(&2));
    }

    #[test]
    fn test_articulation_points_none() {
        // Complete triangle: no AP
        let mut g = Graph::new(false);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        g.add_edge(2, 0, 1.0);
        let ap = articulation_points(&g);
        assert!(ap.is_empty());
    }

    #[test]
    fn test_bridges() {
        // 0--1--2, bridge is 0--1 and 1--2
        let mut g = Graph::new(false);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        let b = bridges(&g);
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn test_bridges_none() {
        let mut g = Graph::new(false);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        g.add_edge(2, 0, 1.0);
        let b = bridges(&g);
        assert!(b.is_empty());
    }

    #[test]
    fn test_bridges_complex() {
        let mut g = Graph::new(false);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        g.add_edge(2, 0, 1.0); // cycle: no bridge
        g.add_edge(2, 3, 1.0); // bridge: 2-3
        g.add_edge(3, 4, 1.0); // bridge: 3-4
        let b = bridges(&g);
        assert_eq!(b.len(), 2);
    }
}
