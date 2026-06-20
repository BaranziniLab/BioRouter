# algo-graph-toolkit-rs

A comprehensive graph-algorithms toolkit library and CLI, written in Rust.

## Features

- Generic directed/undirected weighted graph with adjacency-list storage
- BFS / DFS traversals
- Topological sort
- Connected components (undirected)
- Strongly connected components (Tarjan + Kosaraju)
- Minimum spanning tree (Kruskal + Prim)
- Shortest paths (Dijkstra, Bellman-Ford, Floyd-Warshall)
- Max-flow (Edmonds-Karp)
- Cycle detection, bipartite check, articulation points, bridges
- DOT exporter for visualization
- Edge-list / adjacency file loader
- CLI binary for running algorithms on graph files

## Algorithm / Complexity Table

| Algorithm | Module | Time Complexity | Space |
|---|---|---|---|
| BFS | `traversal` | O(V + E) | O(V) |
| DFS | `traversal` | O(V + E) | O(V) |
| Topological Sort | `toposort` | O(V + E) | O(V) |
| Connected Components | `components` | O(V + E) | O(V) |
| Tarjan SCC | `components` | O(V + E) | O(V) |
| Kosaraju SCC | `components` | O(V + E) | O(V) |
| Kruskal MST | `mst` | O(E log E) | O(V + E) |
| Prim MST | `mst` | O((V + E) log V) | O(V + E) |
| Dijkstra | `shortest_path` | O((V + E) log V) | O(V) |
| Bellman-Ford | `shortest_path` | O(V · E) | O(V) |
| Floyd-Warshall | `shortest_path` | O(V³) | O(V²) |
| Edmonds-Karp (Max-Flow) | `flow` | O(V · E²) | O(V + E) |
| Cycle Detection | `connectivity` | O(V + E) | O(V) |
| Bipartite Check | `connectivity` | O(V + E) | O(V) |
| Articulation Points | `connectivity` | O(V + E) | O(V) |
| Bridges | `connectivity` | O(V + E) | O(V) |

## Usage

### As a library

```rust
use algo_graph_toolkit_rs::graph::Graph;
use algo_graph_toolkit_rs::shortest_path::dijkstra;

let mut g = Graph::new(false);
g.add_edge(0, 1, 4.0);
g.add_edge(0, 2, 1.0);
g.add_edge(2, 1, 2.0);
let (dist, _prev) = dijkstra(&g, 0);
```

### As a CLI

```bash
# Build
cargo build --release

# Run an algorithm on a graph file
cargo run -- run --file graph.txt --algo bfs --source 0
cargo run -- run --file graph.txt --algo dijkstra --source 0
cargo run -- run --file graph.txt --algo mst-kruskal
cargo run -- run --file graph.txt --algo scc-tarjan

# Export to DOT format
cargo run -- export --file graph.txt -o graph.dot

# List available algorithms
cargo run -- list-algos
```

### Graph file format

Edge-list format (one edge per line, weight optional):

```
# comment
# directed  (optional, makes the graph directed)
0 1 5.0
1 2 3.0
2 0 1.0
```

## Running Tests

```bash
cargo test
```

## Running Benchmarks

```bash
cargo bench
```

## Project Structure

```
src/
├── lib.rs             # Module declarations and re-exports
├── main.rs            # CLI entry point
├── graph.rs           # Generic weighted graph (adjacency list)
├── traversal.rs       # BFS, DFS
├── toposort.rs        # Topological sort
├── components.rs      # Connected components, SCC (Tarjan, Kosaraju)
├── mst.rs             # Minimum spanning tree (Kruskal, Prim)
├── shortest_path.rs   # Dijkstra, Bellman-Ford, Floyd-Warshall
├── flow.rs            # Edmonds-Karp max-flow
├── connectivity.rs    # Cycle detection, bipartite, articulation points, bridges
├── io.rs              # DOT exporter, file loader
└── cli.rs             # CLI argument parsing and execution
tests/
└── integration.rs     # Integration tests on known graphs
benches/
└── graph_benchmarks.rs # Criterion benchmarks
```

## License

MIT
