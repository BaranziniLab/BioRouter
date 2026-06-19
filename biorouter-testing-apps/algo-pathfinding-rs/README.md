# algo-pathfinding-rs

A comprehensive pathfinding algorithm library implemented in Rust.

## Features

- **Graph data structures**: Directed/undirected weighted graphs backed by adjacency lists
- **Search algorithms**: BFS, DFS, Dijkstra, A*, Bellman-Ford, Bidirectional BFS
- **Grid support**: Generate grid graphs for 2D pathfinding (4-connected and 8-connected)
- **Heuristic functions**: Manhattan, Euclidean, Chebyshev, and Octile distances
- **Path reconstruction**: Full path result with total cost and node sequence

## Usage

```rust
use algo_pathfinding_rs::graph::AdjacencyListGraph;
use algo_pathfinding_rs::algorithms::dijkstra;
use algo_pathfinding_rs::heuristics;

let mut graph = AdjacencyListGraph::new_undirected();
for i in 0..5 {
    graph.add_node(i);
}
graph.add_edge(0, 1, 4.0);
graph.add_edge(0, 2, 1.0);
graph.add_edge(2, 1, 2.0);
graph.add_edge(1, 3, 5.0);
graph.add_edge(2, 3, 8.0);
graph.add_edge(3, 4, 3.0);

let result = dijkstra(&graph, &0, &4);
assert!(result.is_some());
let path = result.unwrap();
println!("Cost: {}, Path: {:?}", path.total_cost, path.nodes);
```

## Algorithms

| Algorithm | Use Case | Negative Weights | Guarantees |
|-----------|----------|-------------------|------------|
| BFS | Unweighted shortest path | N/A | Optimal (unweighted) |
| DFS | Reachability / cycle detection | N/A | Path found (not shortest) |
| Dijkstra | Single-source shortest path | No | Optimal (non-negative) |
| A* | Directed shortest path | No | Optimal with admissible heuristic |
| Bellman-Ford | Single-source, negative weights | Yes | Optimal or detects negative cycle |
| Bidirectional BFS | Unweighted, large graphs | N/A | Optimal (unweighted) |

## Building

```bash
cargo build
cargo test
```

## License

MIT
