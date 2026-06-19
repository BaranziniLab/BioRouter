//! CLI entry point for algo-graph-toolkit-rs.

use clap::Parser;

use algo_graph_toolkit_rs::cli::{Cli, Command};
use algo_graph_toolkit_rs::io::{load_edge_list, to_dot, write_dot};

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Run {
            file,
            algo,
            source,
            sink,
        } => {
            let graph = match load_edge_list(&file) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("Error loading graph from {}: {}", file.display(), e);
                    std::process::exit(1);
                }
            };
            println!(
                "Loaded graph: {} vertices, {} edges{}",
                graph.vertex_count(),
                graph.edge_count(),
                if graph.directed { " (directed)" } else { " (undirected)" }
            );
            algo_graph_toolkit_rs::cli::run_command(&graph, &algo, source, sink);
        }
        Command::Export { file, output } => {
            let graph = match load_edge_list(&file) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("Error loading graph from {}: {}", file.display(), e);
                    std::process::exit(1);
                }
            };
            match output {
                Some(path) => {
                    if let Err(e) = write_dot(&graph, &path) {
                        eprintln!("Error writing DOT file: {e}");
                        std::process::exit(1);
                    }
                    println!("DOT file written to {}", path.display());
                }
                None => {
                    println!("{}", to_dot(&graph));
                }
            }
        }
        Command::ListAlgos => {
            println!("Available algorithms:");
            println!("  bfs                 - Breadth-first search (--source)");
            println!("  dfs                 - Depth-first search (--source)");
            println!("  toposort            - Topological sort (DFS-based)");
            println!("  toposort-kahn       - Topological sort (Kahn's)");
            println!("  components          - Connected components");
            println!("  scc-tarjan          - SCC (Tarjan's)");
            println!("  scc-kosaraju        - SCC (Kosaraju's)");
            println!("  mst-kruskal         - MST (Kruskal's)");
            println!("  mst-prim            - MST (Prim's)");
            println!("  dijkstra            - Shortest paths (Dijkstra, --source)");
            println!("  bellman-ford        - Shortest paths (Bellman-Ford, --source)");
            println!("  floyd-warshall      - All-pairs shortest paths");
            println!("  max-flow            - Max-flow (Edmonds-Karp, --source, --sink)");
            println!("  cycle-detect        - Cycle detection");
            println!("  bipartite           - Bipartite check");
            println!("  articulation-points - Cut vertices");
            println!("  bridges             - Cut edges");
        }
    }
}
