//! Integration tests: exercise the public API as an external consumer would.

use algo_pathfinding_rs::algorithms::{astar, bellman_ford, bfs, bidirectional_bfs, dfs, dijkstra};
use algo_pathfinding_rs::generators;
use algo_pathfinding_rs::graph::AdjacencyListGraph;
use algo_pathfinding_rs::heuristics;
use algo_pathfinding_rs::path::PathResult;

// ---------------------------------------------------------------------------
// Dijkstra
// ---------------------------------------------------------------------------

#[test]
fn dijkstra_on_weighted_grid() {
    let mut g = AdjacencyListGraph::new_undirected();
    // 3x3 grid with non-uniform weights
    //   0--1--2
    //   |  |  |
    //   3--4--5
    //   |  |  |
    //   6--7--8
    g.add_edge(0, 1, 1.0);
    g.add_edge(1, 2, 1.0);
    g.add_edge(0, 3, 1.0);
    g.add_edge(1, 4, 10.0); // expensive middle edge
    g.add_edge(2, 5, 1.0);
    g.add_edge(3, 4, 1.0);
    g.add_edge(4, 5, 1.0);
    g.add_edge(3, 6, 1.0);
    g.add_edge(4, 7, 1.0);
    g.add_edge(5, 8, 1.0);
    g.add_edge(6, 7, 1.0);
    g.add_edge(7, 8, 1.0);

    let result = dijkstra(&g, &0, &8).unwrap();
    // Optimal path avoids the expensive 1->4 edge: 0-3-6-7-8 cost 4
    assert!((result.total_cost - 4.0).abs() < 1e-9);
}

// ---------------------------------------------------------------------------
// A* with different heuristics
// ---------------------------------------------------------------------------

#[test]
fn astar_manhattan_vs_euclidean_same_cost() {
    let g = generators::grid_4connected(8, 8);
    let start = (0, 0);
    let goal = (7, 7);

    let h_man = |n: &(usize, usize)| {
        heuristics::manhattan(&(n.0 as i32, n.1 as i32), &(goal.0 as i32, goal.1 as i32))
    };
    let h_euc = |n: &(usize, usize)| {
        heuristics::euclidean(&(n.0 as i32, n.1 as i32), &(goal.0 as i32, goal.1 as i32))
    };

    let r1 = astar(&g, &start, &goal, h_man).unwrap();
    let r2 = astar(&g, &start, &goal, h_euc).unwrap();
    assert!((r1.total_cost - r2.total_cost).abs() < 1e-9);
    assert!((r1.total_cost - 14.0).abs() < 1e-9);
}

// ---------------------------------------------------------------------------
// Bellman-Ford with negative edges
// ---------------------------------------------------------------------------

#[test]
fn bellman_ford_negative_edges_shortest() {
    let mut g = AdjacencyListGraph::new_directed();
    g.add_edge(0, 1, 5.0);
    g.add_edge(0, 2, 8.0);
    g.add_edge(1, 2, -3.0); // cheaper via 1
    g.add_edge(2, 3, 2.0);
    g.add_edge(1, 3, 6.0);

    let result = bellman_ford(&g, &0, &3).unwrap().unwrap();
    // 0->1->2->3 = 5 + (-3) + 2 = 4
    assert!((result.total_cost - 4.0).abs() < 1e-9);
}

// ---------------------------------------------------------------------------
// Bidirectional BFS on large graph
// ---------------------------------------------------------------------------

#[test]
fn bidirectional_bfs_large_grid() {
    let g = generators::grid_4connected(50, 50);
    let start = (0, 0);
    let goal = (49, 49);

    let result = bidirectional_bfs(&g, &start, &goal).unwrap();
    // On a 50×50 4-connected grid, shortest hop count = 98
    assert_eq!(result.len(), 98);
}

// ---------------------------------------------------------------------------
// DFS reachability
// ---------------------------------------------------------------------------

#[test]
fn dfs_reachability_in_dag() {
    let mut g = AdjacencyListGraph::new_directed();
    g.add_edge(0, 1, 1.0);
    g.add_edge(1, 2, 1.0);
    g.add_edge(0, 3, 1.0);
    g.add_edge(3, 4, 1.0);
    g.add_edge(4, 2, 1.0);

    // 0 can reach 2 via either path
    assert!(dfs(&g, &0, &2).is_some());
    // 2 cannot reach 0 (directed acyclic)
    assert!(dfs(&g, &2, &0).is_none());
}

// ---------------------------------------------------------------------------
// Path result properties
// ---------------------------------------------------------------------------

#[test]
fn path_result_properties() {
    let p = PathResult {
        nodes: vec![1, 2, 3, 4, 5],
        total_cost: 12.5,
    };
    assert_eq!(p.len(), 4);
    assert!(!p.is_empty());

    let p_single = PathResult {
        nodes: vec![42],
        total_cost: 0.0,
    };
    assert!(p_single.is_empty());
}

// ---------------------------------------------------------------------------
// All algorithms agree on the same unweighted shortest path
// ---------------------------------------------------------------------------

#[test]
fn all_algorithms_agree_on_unweighted_path() {
    let mut g = AdjacencyListGraph::new_undirected();
    g.add_edge(0, 1, 1.0);
    g.add_edge(1, 2, 1.0);
    g.add_edge(0, 2, 1.0);
    g.add_edge(2, 3, 1.0);

    let r_bfs = bfs(&g, &0, &3).unwrap();
    let r_dijk = dijkstra(&g, &0, &3).unwrap();
    let r_astar = astar(&g, &0, &3, |_: &i32| 0.0).unwrap();
    let r_bidir = bidirectional_bfs(&g, &0, &3).unwrap();

    // All should find the same optimal cost (2 hops). BFS returns total_cost=0
    // by design since it ignores weights, so we check hop count instead.
    assert_eq!(r_bfs.len(), 2);
    assert!((r_dijk.total_cost - 2.0).abs() < 1e-9);
    assert!((r_astar.total_cost - 2.0).abs() < 1e-9);
    // Bidirectional returns hop-based paths (total_cost=0 by design), but length is correct
    assert_eq!(r_bidir.len(), 2);
    assert_eq!(r_bfs.len(), r_dijk.len());
}
