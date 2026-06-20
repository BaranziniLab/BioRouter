//! algo-pathfinding-rs: A comprehensive pathfinding algorithm library.
//!
//! # Modules
//!
//! - [`graph`] — Core graph data structures (adjacency list, trait).
//! - [`path`] — Path result type returned by algorithms.
//! - [`algorithms`] — BFS, DFS, Dijkstra, A*, Bellman-Ford, Bidirectional BFS.
//! - [`heuristics`] — Distance heuristics for A* (Manhattan, Euclidean, etc.).
//! - [`generators`] — Pre-built graph topologies (grids, chains, complete graphs).

pub mod algorithms;
pub mod generators;
pub mod graph;
pub mod heuristics;
pub mod path;

#[cfg(test)]
mod lib_tests {
    use crate::algorithms::{astar, bfs, dijkstra};
    use crate::generators;
    use crate::graph::Graph;
    use crate::heuristics;

    #[test]
    fn end_to_end_grid_astar() {
        let g = generators::grid_4connected(10, 10);
        assert_eq!(g.node_count(), 100);

        let start = (0, 0);
        let goal = (9, 9);
        let h = |n: &(usize, usize)| {
            heuristics::manhattan(
                &(n.0 as i32, n.1 as i32),
                &(goal.0 as i32, goal.1 as i32),
            )
        };
        let result = astar(&g, &start, &goal, h).unwrap();
        assert!((result.total_cost - 18.0).abs() < 1e-9);
        assert_eq!(result.len(), 18);
    }

    #[test]
    fn end_to_end_dijkstra_complete() {
        let g = generators::complete_graph(8);
        let result = dijkstra(&g, &0, &7).unwrap();
        assert!(result.total_cost > 0.0);
        assert_eq!(*result.nodes.first().unwrap(), 0);
        assert_eq!(*result.nodes.last().unwrap(), 7);
    }

    #[test]
    fn end_to_end_bfs_chain() {
        let g = generators::chain(100, 1.0);
        let result = bfs(&g, &0, &99).unwrap();
        assert_eq!(result.len(), 99);
        assert_eq!(result.nodes.len(), 100);
    }
}
