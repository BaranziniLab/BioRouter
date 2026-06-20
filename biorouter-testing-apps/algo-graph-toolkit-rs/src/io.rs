//! Graph I/O: DOT export and edge-list/adjacency file loading.

use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

use crate::graph::Graph;

/// Export a graph in DOT format (for Graphviz).
///
/// Returns the DOT string. Use `dot -Tpng graph.dot -o graph.png` to render.
pub fn to_dot(graph: &Graph) -> String {
    let mut out = String::new();
    if graph.directed {
        out.push_str("digraph G {\n");
        out.push_str("    rankdir=LR;\n");
    } else {
        out.push_str("graph G {\n");
        out.push_str("    rankdir=LR;\n");
    }

    let arrow = if graph.directed { "->" } else { "--" };

    for edge in graph.edges() {
        out.push_str(&format!(
            "    {} {} {} [label=\"{}\"];\n",
            edge.src, arrow, edge.dst, edge.weight
        ));
    }
    out.push_str("}\n");
    out
}

/// Write the graph in DOT format to a file.
pub fn write_dot<P: AsRef<Path>>(graph: &Graph, path: P) -> io::Result<()> {
    let dot = to_dot(graph);
    fs::write(path, dot)
}

/// Load a graph from an edge-list file.
///
/// Format:
/// ```text
/// # comment lines start with #
/// # directed           (optional, makes the graph directed)
/// 0 1 5.0              (src dst [weight])
/// 1 2 3.0
/// ```
///
/// - Blank lines and `#` comments are skipped.
/// - The keyword `directed` on its own line makes the graph directed.
/// - Each data line: `src dst [weight]` (weight defaults to 1.0).
pub fn load_edge_list<P: AsRef<Path>>(path: P) -> io::Result<Graph> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut directed = false;
    let mut graph = None;

    for line_result in reader.lines() {
        let line = line_result?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed == "directed" {
            directed = true;
            graph = Some(Graph::new(true));
            continue;
        }

        let g = graph.get_or_insert_with(|| Graph::new(directed));
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let src: usize = parts[0]
            .parse()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let dst: usize = parts[1]
            .parse()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let weight: f64 = if parts.len() > 2 {
            parts[2]
                .parse()
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
        } else {
            1.0
        };
        g.add_edge(src, dst, weight);
    }

    Ok(graph.unwrap_or_else(|| Graph::new(false)))
}

/// Save a graph as an edge-list file (the inverse of `load_edge_list`).
pub fn save_edge_list<P: AsRef<Path>>(graph: &Graph, path: P) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    if graph.directed {
        writeln!(file, "directed")?;
    }
    for edge in graph.edges() {
        writeln!(file, "{} {} {}", edge.src, edge.dst, edge.weight)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_dot_directed() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 5.0);
        g.add_edge(1, 2, 3.0);
        let dot = to_dot(&g);
        assert!(dot.contains("digraph"));
        assert!(dot.contains("->"));
        assert!(dot.contains("5"));
    }

    #[test]
    fn test_dot_undirected() {
        let mut g = Graph::new(false);
        g.add_edge(0, 1, 2.0);
        let dot = to_dot(&g);
        assert!(dot.contains("graph"));
        assert!(dot.contains("--"));
    }

    #[test]
    fn test_load_edge_list() {
        let content = "# test graph\ndirected\n0 1 5.0\n1 2 3.0\n2 0 1.0\n";
        let path = "/tmp/test_graph.txt";
        fs::write(path, content).unwrap();
        let g = load_edge_list(path).unwrap();
        assert!(g.directed);
        assert_eq!(g.vertex_count(), 3);
        assert_eq!(g.edge_count(), 3);
    }

    #[test]
    fn test_load_undirected() {
        let content = "0 1 2.0\n1 2 3.0\n";
        let path = "/tmp/test_graph_undir.txt";
        fs::write(path, content).unwrap();
        let g = load_edge_list(path).unwrap();
        assert!(!g.directed);
        assert_eq!(g.vertex_count(), 3);
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let mut g = Graph::new(true);
        g.add_edge(0, 1, 5.0);
        g.add_edge(1, 2, 3.0);
        let path = "/tmp/test_roundtrip.txt";
        save_edge_list(&g, path).unwrap();
        let g2 = load_edge_list(path).unwrap();
        assert_eq!(g2.vertex_count(), 3);
        assert_eq!(g2.edge_count(), 2);
    }

    #[test]
    fn test_load_default_weight() {
        let content = "0 1\n1 2\n";
        let path = "/tmp/test_default_weight.txt";
        fs::write(path, content).unwrap();
        let g = load_edge_list(path).unwrap();
        for edge in g.edges() {
            assert!((edge.weight - 1.0).abs() < 1e-9);
        }
    }
}
