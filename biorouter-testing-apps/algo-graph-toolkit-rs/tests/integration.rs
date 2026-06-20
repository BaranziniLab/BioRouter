//! Integration tests on known graphs.

use algo_graph_toolkit_rs::components::{connected_components, kosaraju_scc, tarjan_scc};
use algo_graph_toolkit_rs::connectivity::{articulation_points, bridges, has_cycle, is_bipartite};
use algo_graph_toolkit_rs::flow::edmonds_karp;
use algo_graph_toolkit_rs::graph::Graph;
use algo_graph_toolkit_rs::io::{load_edge_list, save_edge_list, to_dot};
use algo_graph_toolkit_rs::mst::{kruskal, prim};
use algo_graph_toolkit_rs::shortest_path::{bellman_ford, dijkstra, floyd_warshall, reconstruct_path};
use algo_graph_toolkit_rs::toposort::{topological_sort, topological_sort_kahn};
use algo_graph_toolkit_rs::traversal::{bfs, dfs};

// ─────────────────────────────────────────────────────────────
// Helper: build the classic CLRS-style graph for Dijkstra tests
// ─────────────────────────────────────────────────────────────
fn clrs_graph() -> Graph {
    let mut g = Graph::new(true);
    g.add_edge(0, 1, 10.0);
    g.add_edge(0, 2, 3.0);
    g.add_edge(1, 2, 1.0);
    g.add_edge(1, 3, 2.0);
    g.add_edge(2, 1, 4.0);
    g.add_edge(2, 3, 8.0);
    g.add_edge(2, 4, 2.0);
    g.add_edge(3, 4, 7.0);
    g.add_edge(4, 3, 9.0);
    g
}

// ─────────────────────────────────────────────────────────────
// CLRS Dijkstra
// ─────────────────────────────────────────────────────────────
#[test]
fn integration_dijkstra_clrs() {
    let g = clrs_graph();
    let (dist, prev) = dijkstra(&g, 0);
    assert_eq!(dist[0], 0.0);
    assert_eq!(dist[1], 7.0);
    assert_eq!(dist[2], 3.0);
    assert_eq!(dist[3], 9.0);
    assert_eq!(dist[4], 5.0);

    let path = reconstruct_path(&prev, 0, 3).unwrap();
    assert_eq!(path, vec![0, 2, 1, 3]);
}

// ─────────────────────────────────────────────────────────────
// Bellman-Ford: negative edges, no cycle
// ─────────────────────────────────────────────────────────────
#[test]
fn integration_bellman_ford_negative() {
    let mut g = Graph::new(true);
    g.add_edge(0, 1, 5.0);
    g.add_edge(0, 2, 8.0);
    g.add_edge(1, 2, -3.0);
    let (dist, _) = bellman_ford(&g, 0).unwrap();
    assert_eq!(dist[0], 0.0);
    assert_eq!(dist[1], 5.0);
    assert_eq!(dist[2], 2.0);
}

// ─────────────────────────────────────────────────────────────
// Bellman-Ford: negative cycle
// ─────────────────────────────────────────────────────────────
#[test]
fn integration_bellman_ford_negative_cycle() {
    let mut g = Graph::new(true);
    g.add_edge(0, 1, 1.0);
    g.add_edge(1, 2, -3.0);
    g.add_edge(2, 0, 1.0);
    assert!(bellman_ford(&g, 0).is_err());
}

// ─────────────────────────────────────────────────────────────
// Floyd-Warshall
// ─────────────────────────────────────────────────────────────
#[test]
fn integration_floyd_warshall_triangle() {
    let mut g = Graph::new(true);
    g.add_edge(0, 1, 1.0);
    g.add_edge(1, 2, 2.0);
    g.add_edge(0, 2, 4.0);
    let dist = floyd_warshall(&g);
    // Direct 0→2 = 4, but 0→1→2 = 3
    assert_eq!(dist[0][2], 3.0);
    assert_eq!(dist[0][1], 1.0);
    assert_eq!(dist[1][2], 2.0);
}

// ─────────────────────────────────────────────────────────────
// BFS/DFS on a tree
// ─────────────────────────────────────────────────────────────
#[test]
fn integration_bfs_tree() {
    let mut g = Graph::new(false);
    g.add_edge(0, 1, 1.0);
    g.add_edge(0, 2, 1.0);
    g.add_edge(1, 3, 1.0);
    g.add_edge(1, 4, 1.0);
    let order = bfs(&g, 0);
    assert_eq!(order[0], 0);
    assert_eq!(order.len(), 5);
}

#[test]
fn integration_dfs_tree() {
    let mut g = Graph::new(false);
    g.add_edge(0, 1, 1.0);
    g.add_edge(0, 2, 1.0);
    g.add_edge(1, 3, 1.0);
    let order = dfs(&g, 0);
    assert_eq!(order[0], 0);
    assert_eq!(order.len(), 4);
}

// ─────────────────────────────────────────────────────────────
// Topological sort: textbook DAG
// ─────────────────────────────────────────────────────────────
#[test]
fn integration_toposort_textbook() {
    // socks → shoes, shirt → belt, shirt → tie, tie → jacket,
    // belt → jacket, pants → belt, pants → shoes
    let mut g = Graph::new(true);
    g.add_edge(0, 1, 1.0); // socks -> shoes
    g.add_edge(2, 3, 1.0); // shirt -> belt
    g.add_edge(2, 4, 1.0); // shirt -> tie
    g.add_edge(4, 5, 1.0); // tie -> jacket
    g.add_edge(3, 5, 1.0); // belt -> jacket
    g.add_edge(6, 3, 1.0); // pants -> belt
    g.add_edge(6, 1, 1.0); // pants -> shoes

    let order = topological_sort(&g).unwrap();
    assert_eq!(order.len(), 7);

    fn pos(order: &[usize], v: usize) -> usize {
        order.iter().position(|&x| x == v).unwrap()
    }
    // Verify ordering constraints
    assert!(pos(&order, 0) < pos(&order, 1));
    assert!(pos(&order, 2) < pos(&order, 3));
    assert!(pos(&order, 2) < pos(&order, 4));
    assert!(pos(&order, 4) < pos(&order, 5));
    assert!(pos(&order, 3) < pos(&order, 5));
    assert!(pos(&order, 6) < pos(&order, 3));
    assert!(pos(&order, 6) < pos(&order, 1));
}

#[test]
fn integration_kahn_agrees_with_dfs() {
    let mut g = Graph::new(true);
    g.add_edge(5, 2, 1.0);
    g.add_edge(5, 0, 1.0);
    g.add_edge(4, 0, 1.0);
    g.add_edge(4, 1, 1.0);
    g.add_edge(2, 3, 1.0);
    g.add_edge(3, 1, 1.0);

    let o1 = topological_sort(&g).unwrap();
    let o2 = topological_sort_kahn(&g).unwrap();
    assert_eq!(o1.len(), o2.len());

    // Both must satisfy edge constraints
    fn check_order(g: &Graph, order: &[usize]) {
        let pos: Vec<usize> = order.to_vec();
        for edge in g.edges() {
            let pi = order.iter().position(|&x| x == edge.src).unwrap();
            let pj = order.iter().position(|&x| x == edge.dst).unwrap();
            assert!(pi < pj, "{} should come before {}", edge.src, edge.dst);
        }
    }
    check_order(&g, &o1);
    check_order(&g, &o2);
}

// ─────────────────────────────────────────────────────────────
// Connected components: disconnected graph
// ─────────────────────────────────────────────────────────────
#[test]
fn integration_connected_components_disconnected() {
    let mut g = Graph::new(false);
    // Component 1: triangle
    g.add_edge(0, 1, 1.0);
    g.add_edge(1, 2, 1.0);
    g.add_edge(2, 0, 1.0);
    // Component 2: edge
    g.add_edge(3, 4, 1.0);
    // Component 3: isolated vertex
    g.add_vertex(100);

    let cc = connected_components(&g);
    assert_eq!(cc.len(), 3);
    let sizes: Vec<usize> = cc.iter().map(|c| c.len()).collect();
    let mut sorted_sizes = sizes.clone();
    sorted_sizes.sort();
    assert_eq!(sorted_sizes, vec![1, 2, 3]);
}

// ─────────────────────────────────────────────────────────────
// SCC: Tarjan and Kosaraju agree
// ─────────────────────────────────────────────────────────────
#[test]
fn integration_scc_tarjan_kosaraju_agree() {
    // Classic SCC example:
    // 0→1→2→0, 2→3, 3→4→5→3
    let mut g = Graph::new(true);
    g.add_edge(0, 1, 1.0);
    g.add_edge(1, 2, 1.0);
    g.add_edge(2, 0, 1.0);
    g.add_edge(2, 3, 1.0);
    g.add_edge(3, 4, 1.0);
    g.add_edge(4, 5, 1.0);
    g.add_edge(5, 3, 1.0);

    let mut t = tarjan_scc(&g);
    let mut k = kosaraju_scc(&g);
    // Normalize: sort each SCC internally, then sort the list of SCCs
    for scc in t.iter_mut() {
        scc.sort();
    }
    t.sort();
    for scc in k.iter_mut() {
        scc.sort();
    }
    k.sort();

    assert_eq!(t, k);
    assert_eq!(t.len(), 2);
}

// ─────────────────────────────────────────────────────────────
// MST: classic 4-node graph
// ─────────────────────────────────────────────────────────────
#[test]
fn integration_mst_classic() {
    let mut g = Graph::new(false);
    g.add_edge(0, 1, 10.0);
    g.add_edge(0, 2, 6.0);
    g.add_edge(0, 3, 5.0);
    g.add_edge(1, 3, 15.0);
    g.add_edge(2, 3, 4.0);

    let (k_edges, k_total) = kruskal(&g);
    let (p_edges, p_total) = prim(&g);
    assert!((k_total - p_total).abs() < 1e-9);
    assert_eq!(k_edges.len(), 3); // V-1
    assert_eq!(p_edges.len(), 3);
    assert!((k_total - 19.0).abs() < 1e-9); // 4+5+10
}

// ─────────────────────────────────────────────────────────────
// Max-flow: textbook 6-node network
// ─────────────────────────────────────────────────────────────
#[test]
fn integration_max_flow_textbook() {
    let mut g = Graph::new(true);
    g.add_edge(0, 1, 16.0);
    g.add_edge(0, 2, 13.0);
    g.add_edge(1, 2, 4.0);
    g.add_edge(1, 3, 12.0);
    g.add_edge(2, 1, 10.0);
    g.add_edge(2, 4, 14.0);
    g.add_edge(3, 2, 9.0);
    g.add_edge(3, 5, 20.0);
    g.add_edge(4, 3, 7.0);
    g.add_edge(4, 5, 4.0);

    let (flow, _) = edmonds_karp(&g, 0, 5);
    assert!((flow - 23.0).abs() < 1e-9);
}

// ─────────────────────────────────────────────────────────────
// Cycle detection
// ─────────────────────────────────────────────────────────────
#[test]
fn integration_cycle_detection_directed() {
    let mut g = Graph::new(true);
    g.add_edge(0, 1, 1.0);
    g.add_edge(1, 2, 1.0);
    assert!(!has_cycle(&g));
    g.add_edge(2, 0, 1.0);
    assert!(has_cycle(&g));
}

#[test]
fn integration_cycle_detection_undirected() {
    let mut g = Graph::new(false);
    g.add_edge(0, 1, 1.0);
    g.add_edge(1, 2, 1.0);
    g.add_edge(2, 3, 1.0);
    assert!(!has_cycle(&g));
    g.add_edge(3, 0, 1.0);
    assert!(has_cycle(&g));
}

// ─────────────────────────────────────────────────────────────
// Bipartite
// ─────────────────────────────────────────────────────────────
#[test]
fn integration_bipartite_square() {
    let mut g = Graph::new(false);
    g.add_edge(0, 1, 1.0);
    g.add_edge(1, 2, 1.0);
    g.add_edge(2, 3, 1.0);
    g.add_edge(3, 0, 1.0);
    assert!(is_bipartite(&g).is_some());
}

#[test]
fn integration_bipartite_triangle_fails() {
    let mut g = Graph::new(false);
    g.add_edge(0, 1, 1.0);
    g.add_edge(1, 2, 1.0);
    g.add_edge(2, 0, 1.0);
    assert!(is_bipartite(&g).is_none());
}

// ─────────────────────────────────────────────────────────────
// Articulation points and bridges
// ─────────────────────────────────────────────────────────────
#[test]
fn integration_articulation_and_bridges() {
    // Graph: 0-1-2-3, 1-4, 2-5, 4-5
    // APs: {1, 2}, Bridges: {1-3? no}, let's use a cleaner example
    // Bridge graph: 0-1-2, with extra edge 0-2
    let mut g = Graph::new(false);
    g.add_edge(0, 1, 1.0);
    g.add_edge(1, 2, 1.0);
    g.add_edge(2, 3, 1.0);

    let ap = articulation_points(&g);
    // 1 and 2 are articulation points in a chain 0-1-2-3
    assert!(ap.contains(&1));
    assert!(ap.contains(&2));
    assert!(!ap.contains(&0));
    assert!(!ap.contains(&3));

    let b = bridges(&g);
    assert_eq!(b.len(), 3); // all three edges are bridges
}

// ─────────────────────────────────────────────────────────────
// I/O: round-trip through file
// ─────────────────────────────────────────────────────────────
#[test]
fn integration_io_roundtrip() {
    let mut g = Graph::new(true);
    g.add_edge(0, 1, 5.0);
    g.add_edge(1, 2, 3.0);
    g.add_edge(2, 0, 1.0);

    let path = "/tmp/agtk_integration_test.txt";
    save_edge_list(&g, path).unwrap();
    let g2 = load_edge_list(path).unwrap();

    assert_eq!(g2.vertex_count(), 3);
    assert_eq!(g2.edge_count(), 3);

    // Check weights
    for edge in g.edges() {
        let found = g2
            .edges()
            .iter()
            .any(|e| e.src == edge.src && e.dst == edge.dst && (e.weight - edge.weight).abs() < 1e-9);
        assert!(found, "Missing edge: {:?}", edge);
    }
}

// ─────────────────────────────────────────────────────────────
// DOT export
// ─────────────────────────────────────────────────────────────
#[test]
fn integration_dot_export() {
    let mut g = Graph::new(false);
    g.add_edge(0, 1, 2.5);
    g.add_edge(1, 2, 3.0);
    let dot = to_dot(&g);
    assert!(dot.contains("graph G"));
    assert!(dot.contains("2.5"));
}

// ─────────────────────────────────────────────────────────────
// Single vertex graph
// ─────────────────────────────────────────────────────────────
#[test]
fn integration_single_vertex() {
    let mut g = Graph::new(true);
    g.add_vertex(42);
    let order = bfs(&g, 42);
    assert_eq!(order, vec![42]);

    let order = dfs(&g, 42);
    assert_eq!(order, vec![42]);

    let cc = connected_components(&g);
    assert_eq!(cc.len(), 1);

    assert!(!has_cycle(&g));
    assert!(is_bipartite(&g).is_some());
}

// ─────────────────────────────────────────────────────────────
// Empty graph
// ─────────────────────────────────────────────────────────────
#[test]
fn integration_empty_graph() {
    let g = Graph::new(true);
    let cc = connected_components(&g);
    assert!(cc.is_empty());
    assert!(!has_cycle(&g));
    let dist = floyd_warshall(&g);
    assert!(dist.is_empty());
}
