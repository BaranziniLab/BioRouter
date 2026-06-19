//! CLI argument parsing and execution.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::graph::Graph;

#[derive(Parser)]
#[command(name = "algo-graph-toolkit-rs")]
#[command(about = "A graph-algorithms toolkit library and CLI in Rust")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Run an algorithm on a graph file
    Run {
        /// Path to the graph file (edge-list format)
        #[arg(short, long)]
        file: PathBuf,

        /// Algorithm to run
        #[arg(short, long)]
        algo: Algorithm,

        /// Source vertex (for BFS, DFS, Dijkstra, Bellman-Ford, max-flow source)
        #[arg(long)]
        source: Option<usize>,

        /// Sink vertex (for max-flow)
        #[arg(long)]
        sink: Option<usize>,
    },

    /// Export a graph file to DOT format for visualization
    Export {
        /// Path to the graph file
        #[arg(short, long)]
        file: PathBuf,

        /// Output path (stdout if omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// List all available algorithms
    ListAlgos,
}

#[derive(Clone, ValueEnum)]
pub enum Algorithm {
    /// Breadth-first search
    Bfs,
    /// Depth-first search
    Dfs,
    /// Topological sort (DFS-based)
    Toposort,
    /// Topological sort (Kahn's algorithm)
    ToposortKahn,
    /// Connected components
    Components,
    /// Strongly connected components (Tarjan)
    SccTarjan,
    /// Strongly connected components (Kosaraju)
    SccKosaraju,
    /// Minimum spanning tree (Kruskal)
    MstKruskal,
    /// Minimum spanning tree (Prim)
    MstPrim,
    /// Dijkstra shortest paths
    Dijkstra,
    /// Bellman-Ford shortest paths
    BellmanFord,
    /// Floyd-Warshall all-pairs shortest paths
    FloydWarshall,
    /// Max-flow (Edmonds-Karp)
    MaxFlow,
    /// Cycle detection
    CycleDetect,
    /// Bipartite check
    Bipartite,
    /// Articulation points
    ArticulationPoints,
    /// Bridges
    Bridges,
}

pub fn run_command(graph: &Graph, algo: &Algorithm, source: Option<usize>, sink: Option<usize>) {
    match algo {
        Algorithm::Bfs => {
            let src = source.unwrap_or(0);
            let order = crate::traversal::bfs(graph, src);
            println!("BFS from vertex {src}:");
            println!("  Order: {:?}", order);
        }
        Algorithm::Dfs => {
            let src = source.unwrap_or(0);
            let order = crate::traversal::dfs(graph, src);
            println!("DFS from vertex {src}:");
            println!("  Order: {:?}", order);
        }
        Algorithm::Toposort => {
            match crate::toposort::topological_sort(graph) {
                Some(order) => {
                    println!("Topological sort (DFS):");
                    println!("  Order: {:?}", order);
                }
                None => {
                    println!("Error: graph contains a cycle (or is undirected).");
                }
            }
        }
        Algorithm::ToposortKahn => {
            match crate::toposort::topological_sort_kahn(graph) {
                Some(order) => {
                    println!("Topological sort (Kahn):");
                    println!("  Order: {:?}", order);
                }
                None => {
                    println!("Error: graph contains a cycle (or is undirected).");
                }
            }
        }
        Algorithm::Components => {
            let cc = crate::components::connected_components(graph);
            println!("Connected components ({} found):", cc.len());
            for (i, comp) in cc.iter().enumerate() {
                println!("  Component {}: {:?}", i, comp);
            }
        }
        Algorithm::SccTarjan => {
            let sccs = crate::components::tarjan_scc(graph);
            println!("Strongly connected components (Tarjan, {} found):", sccs.len());
            for (i, scc) in sccs.iter().enumerate() {
                println!("  SCC {}: {:?}", i, scc);
            }
        }
        Algorithm::SccKosaraju => {
            let sccs = crate::components::kosaraju_scc(graph);
            println!("Strongly connected components (Kosaraju, {} found):", sccs.len());
            for (i, scc) in sccs.iter().enumerate() {
                println!("  SCC {}: {:?}", i, scc);
            }
        }
        Algorithm::MstKruskal => {
            let (mst, total) = crate::mst::kruskal(graph);
            println!("MST (Kruskal): total weight = {total}");
            for edge in &mst {
                println!("  {} -- {}  (weight {})", edge.src, edge.dst, edge.weight);
            }
        }
        Algorithm::MstPrim => {
            let (mst, total) = crate::mst::prim(graph);
            println!("MST (Prim): total weight = {total}");
            for edge in &mst {
                println!("  {} -- {}  (weight {})", edge.src, edge.dst, edge.weight);
            }
        }
        Algorithm::Dijkstra => {
            let src = source.unwrap_or(0);
            let (dist, prev) = crate::shortest_path::dijkstra(graph, src);
            println!("Dijkstra from vertex {src}:");
            for (v, &d) in dist.iter().enumerate() {
                if d < f64::INFINITY {
                    let path = crate::shortest_path::reconstruct_path(&prev, src, v);
                    println!("  {v}: distance = {d}, path = {:?}", path.unwrap_or_default());
                }
            }
        }
        Algorithm::BellmanFord => {
            let src = source.unwrap_or(0);
            match crate::shortest_path::bellman_ford(graph, src) {
                Ok((dist, prev)) => {
                    println!("Bellman-Ford from vertex {src}:");
                    for (v, &d) in dist.iter().enumerate() {
                        if d < f64::INFINITY {
                            let path = crate::shortest_path::reconstruct_path(&prev, src, v);
                            println!("  {v}: distance = {d}, path = {:?}", path.unwrap_or_default());
                        }
                    }
                }
                Err(()) => {
                    println!("Error: negative-weight cycle detected.");
                }
            }
        }
        Algorithm::FloydWarshall => {
            let dist = crate::shortest_path::floyd_warshall(graph);
            println!("Floyd-Warshall all-pairs shortest paths:");
            for (i, row) in dist.iter().enumerate() {
                for (j, &d) in row.iter().enumerate() {
                    if d < f64::INFINITY {
                        print!("  {i}->{j}: {d:.1}");
                    }
                }
                if row.iter().any(|&d| d < f64::INFINITY) {
                    println!();
                }
            }
        }
        Algorithm::MaxFlow => {
            let src = source.unwrap_or(0);
            let snk = sink.unwrap_or_else(|| {
                let vertices: Vec<usize> = graph.vertices().collect();
                *vertices.last().unwrap_or(&0)
            });
            let (flow, residual) = crate::flow::edmonds_karp(graph, src, snk);
            println!("Max flow ({src} -> {snk}): {flow}");
            let flows = crate::flow::extract_flow(graph, &residual);
            for (u, v, f) in &flows {
                println!("  {u} -> {v}: flow = {f}");
            }
        }
        Algorithm::CycleDetect => {
            let has = crate::connectivity::has_cycle(graph);
            if has {
                println!("Graph contains a cycle.");
            } else {
                println!("Graph is acyclic.");
            }
        }
        Algorithm::Bipartite => {
            match crate::connectivity::is_bipartite(graph) {
                Some((a, b)) => {
                    println!("Graph is bipartite.");
                    println!("  Set A: {:?}", a);
                    println!("  Set B: {:?}", b);
                }
                None => {
                    println!("Graph is NOT bipartite.");
                }
            }
        }
        Algorithm::ArticulationPoints => {
            let ap = crate::connectivity::articulation_points(graph);
            println!("Articulation points ({} found):", ap.len());
            let mut sorted: Vec<usize> = ap.into_iter().collect();
            sorted.sort();
            for v in sorted {
                println!("  Vertex {v}");
            }
        }
        Algorithm::Bridges => {
            let b = crate::connectivity::bridges(graph);
            println!("Bridges ({} found):", b.len());
            for (u, v) in &b {
                println!("  {u} -- {v}");
            }
        }
    }
}
