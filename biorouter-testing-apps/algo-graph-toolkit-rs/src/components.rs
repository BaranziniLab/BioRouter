//! Connected components (undirected) and strongly connected components (Tarjan + Kosaraju).

use std::collections::{HashSet, VecDeque};

use crate::graph::Graph;
use crate::traversal::dfs_finish_times;

/// Find connected components of an undirected graph.
/// Returns a `Vec` of components, each a `Vec<usize>` of vertex IDs.
pub fn connected_components(graph: &Graph) -> Vec<Vec<usize>> {
    let mut visited = HashSet::new();
    let mut components = Vec::new();

    for v in graph.vertices() {
        if visited.contains(&v) {
            continue;
        }
        let mut component = Vec::new();
        let mut queue = VecDeque::new();
        visited.insert(v);
        queue.push_back(v);

        while let Some(node) = queue.pop_front() {
            component.push(node);
            for &(dst, _) in graph.neighbours(node) {
                if visited.insert(dst) {
                    queue.push_back(dst);
                }
            }
        }
        components.push(component);
    }
    components
}

/// Strongly connected components using Tarjan's algorithm.
/// Returns components in reverse topological order of the SCC DAG.
pub fn tarjan_scc(graph: &Graph) -> Vec<Vec<usize>> {
    let vertices: Vec<usize> = graph.vertices().collect();
    if vertices.is_empty() {
        return Vec::new();
    }
    let max_v = *vertices.iter().max().unwrap() + 1;

    let mut index = 0usize;
    let mut indices = vec![usize::MAX; max_v];
    let mut lowlink = vec![0usize; max_v];
    let mut on_stack = vec![false; max_v];
    let mut stack: Vec<usize> = Vec::new();
    let mut sccs: Vec<Vec<usize>> = Vec::new();

    // Iterative Tarjan using explicit call stack
    // Stack entries: (vertex, neighbour_index, is_return)
    let mut call_stack: Vec<(usize, usize, bool)> = Vec::new();

    for &start in &vertices {
        if indices[start] != usize::MAX {
            continue;
        }
        call_stack.push((start, 0, false));

        while let Some((v, ni, is_return)) = call_stack.pop() {
            if is_return {
                // Returning from processing a neighbour
                let parent = call_stack.last().map(|&(p, _, _)| p);
                if let Some(parent_v) = parent {
                    // Update lowlink of parent
                    if lowlink[v] < lowlink[parent_v] {
                        // We need to update parent's lowlink but parent is on call_stack
                        // Actually, we handle this differently in the iterative approach
                    }
                }
                // Update lowlink of the vertex that called us
                // This is tricky in iterative - let me use recursive with increased stack
                continue;
            }

            if indices[v] == usize::MAX {
                indices[v] = index;
                lowlink[v] = index;
                index += 1;
                stack.push(v);
                on_stack[v] = true;
            }

            let neighbours: Vec<usize> = graph
                .neighbours(v)
                .iter()
                .map(|&(d, _)| d)
                .collect();

            let mut done = true;
            for i in ni..neighbours.len() {
                let w = neighbours[i];
                if indices[w] == usize::MAX {
                    // Not yet visited: recurse
                    call_stack.push((v, i + 1, false));
                    call_stack.push((w, 0, false));
                    done = false;
                    break;
                } else if on_stack[w] {
                    if indices[w] < lowlink[v] {
                        lowlink[v] = indices[w];
                    }
                }
            }

            if done {
                // All neighbours processed
                if lowlink[v] == indices[v] {
                    // Root of an SCC
                    let mut scc = Vec::new();
                    loop {
                        let w = stack.pop().unwrap();
                        on_stack[w] = false;
                        scc.push(w);
                        if w == v {
                            break;
                        }
                    }
                    sccs.push(scc);
                }
                // Update parent's lowlink
                if let Some(&(parent_v, _, _)) = call_stack.last() {
                    if lowlink[v] < lowlink[parent_v] {
                        lowlink[parent_v] = lowlink[v];
                    }
                }
            }
        }
    }
    sccs
}

/// Strongly connected components using Kosaraju's algorithm.
/// Returns components (order is implementation-dependent).
pub fn kosaraju_scc(graph: &Graph) -> Vec<Vec<usize>> {
    let vertices: Vec<usize> = graph.vertices().collect();
    if vertices.is_empty() {
        return Vec::new();
    }

    // Step 1: Get finish times from DFS on original graph
    let (_discovery, finish_order) = dfs_finish_times(graph);

    // Step 2: DFS on reversed graph in decreasing finish time order
    let rev = graph.reverse();
    let max_v = *vertices.iter().max().unwrap() + 1;
    let mut visited = vec![false; max_v];
    let mut sccs = Vec::new();

    // finish_order is in first-finished-first order; Kosaraju needs
    // decreasing finish time (last-finished-first), so iterate in reverse.
    for &start in finish_order.iter().rev() {
        if visited[start] {
            continue;
        }
        let mut scc = Vec::new();
        let mut stack = vec![start];
        while let Some(v) = stack.pop() {
            if visited[v] {
                continue;
            }
            visited[v] = true;
            scc.push(v);
            for &(dst, _) in rev.neighbours(v) {
                if !visited[dst] {
                    stack.push(dst);
                }
            }
        }
        sccs.push(scc);
    }
    sccs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connected_components_single() {
        let mut g = Graph::new(false);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        let cc = connected_components(&g);
        assert_eq!(cc.len(), 1);
        assert_eq!(cc[0].len(), 3);
    }

    #[test]
    fn test_connected_components_disconnected() {
        let mut g = Graph::new(false);
        g.add_edge(0, 1, 1.0);
        g.add_vertex(5);
        g.add_vertex(6);
        g.add_edge(5, 6, 1.0);
        let cc = connected_components(&g);
        assert_eq!(cc.len(), 2);
    }

    #[test]
    fn test_connected_components_isolated() {
        let mut g = Graph::new(false);
        g.add_vertex(0);
        g.add_vertex(1);
        g.add_vertex(2);
        let cc = connected_components(&g);
        assert_eq!(cc.len(), 3);
    }

    #[test]
    fn test_tarjan_scc_simple_cycle() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        g.add_edge(2, 0, 1.0);
        let sccs = tarjan_scc(&g);
        assert_eq!(sccs.len(), 1);
        let mut s = sccs[0].clone();
        s.sort();
        assert_eq!(s, vec![0, 1, 2]);
    }

    #[test]
    fn test_tarjan_scc_two_components() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 0, 1.0);
        g.add_edge(2, 3, 1.0);
        g.add_edge(3, 2, 1.0);
        let sccs = tarjan_scc(&g);
        assert_eq!(sccs.len(), 2);
    }

    #[test]
    fn test_tarjan_scc_dag() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        let sccs = tarjan_scc(&g);
        assert_eq!(sccs.len(), 3);
    }

    #[test]
    fn test_kosaraju_scc_simple_cycle() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        g.add_edge(2, 0, 1.0);
        let sccs = kosaraju_scc(&g);
        assert_eq!(sccs.len(), 1);
        let mut s = sccs[0].clone();
        s.sort();
        assert_eq!(s, vec![0, 1, 2]);
    }

    #[test]
    fn test_kosaraju_scc_two_components() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 0, 1.0);
        g.add_edge(2, 3, 1.0);
        g.add_edge(3, 2, 1.0);
        let sccs = kosaraju_scc(&g);
        assert_eq!(sccs.len(), 2);
    }

    #[test]
    fn test_kosaraju_scc_complex() {
        // Classic example: 0→1→2→0, 2→3→4→3
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 1.0);
        g.add_edge(1, 2, 1.0);
        g.add_edge(2, 0, 1.0);
        g.add_edge(2, 3, 1.0);
        g.add_edge(3, 4, 1.0);
        g.add_edge(4, 3, 1.0);
        let sccs = kosaraju_scc(&g);
        assert_eq!(sccs.len(), 2);
    }
}
