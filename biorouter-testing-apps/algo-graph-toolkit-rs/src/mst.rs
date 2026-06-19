//! Minimum spanning tree: Kruskal (Union-Find) and Prim (priority queue).

use std::collections::BinaryHeap;
use std::cmp::Reverse;

use crate::graph::{Edge, Graph};

/// Union-Find (Disjoint Set Union) with path compression and union by rank.
#[derive(Debug)]
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        UnionFind {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    fn union(&mut self, x: usize, y: usize) -> bool {
        let rx = self.find(x);
        let ry = self.find(y);
        if rx == ry {
            return false;
        }
        if self.rank[rx] < self.rank[ry] {
            self.parent[rx] = ry;
        } else if self.rank[rx] > self.rank[ry] {
            self.parent[ry] = rx;
        } else {
            self.parent[ry] = rx;
            self.rank[rx] += 1;
        }
        true
    }
}

/// Kruskal's algorithm for minimum spanning tree.
///
/// Returns `(mst_edges, total_weight)`. For undirected graphs only.
/// If the graph is disconnected, returns an MST forest.
pub fn kruskal(graph: &Graph) -> (Vec<Edge>, f64) {
    let vertices: Vec<usize> = graph.vertices().collect();
    if vertices.is_empty() {
        return (Vec::new(), 0.0);
    }
    let max_v = *vertices.iter().max().unwrap();
    let mut edges = graph.edges();
    edges.sort_by(|a, b| a.weight.partial_cmp(&b.weight).unwrap());

    let mut uf = UnionFind::new(max_v + 1);
    let mut mst = Vec::new();
    let mut total = 0.0;

    for edge in edges {
        if uf.union(edge.src, edge.dst) {
            total += edge.weight;
            mst.push(edge);
        }
    }
    (mst, total)
}

/// Prim's algorithm for minimum spanning tree.
///
/// Returns `(mst_edges, total_weight)`. For undirected graphs only.
/// If the graph is disconnected, returns an MST forest that spans
/// every connected component.
pub fn prim(graph: &Graph) -> (Vec<Edge>, f64) {
    let vertices: Vec<usize> = graph.vertices().collect();
    if vertices.is_empty() {
        return (Vec::new(), 0.0);
    }

    let mut in_mst = std::collections::HashSet::new();
    let mut mst = Vec::new();
    let mut total = 0.0;

    // Iterate over all vertices so disconnected components are handled.
    for &root in &vertices {
        if in_mst.contains(&root) {
            continue;
        }
        // Min-heap: (weight_encoded, src, dst)
        let mut heap: BinaryHeap<Reverse<(i64, usize, usize)>> = BinaryHeap::new();
        in_mst.insert(root);
        for &(dst, w) in graph.neighbours(root) {
            heap.push(Reverse(((w * 1000.0) as i64, root, dst)));
        }

        while let Some(Reverse((_, src, dst))) = heap.pop() {
            if in_mst.contains(&dst) {
                continue;
            }
            in_mst.insert(dst);
            let weight = graph
                .neighbours(src)
                .iter()
                .find(|&&(d, _)| d == dst)
                .map(|&(_, w)| w)
                .unwrap_or(0.0);
            total += weight;
            mst.push(Edge { src, dst, weight });

            for &(next, w) in graph.neighbours(dst) {
                if !in_mst.contains(&next) {
                    heap.push(Reverse(((w * 1000.0) as i64, dst, next)));
                }
            }
        }
    }
    (mst, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_graph() -> Graph {
        let mut g = Graph::new(false);
        g.add_edge(0, 1, 4.0);
        g.add_edge(0, 2, 1.0);
        g.add_edge(1, 2, 2.0);
        g.add_edge(1, 3, 5.0);
        g.add_edge(2, 3, 8.0);
        g
    }

    #[test]
    fn test_kruskal_simple() {
        let g = sample_graph();
        let (mst, total) = kruskal(&g);
        assert_eq!(mst.len(), 3); // V-1 edges for connected graph
        assert!((total - 8.0).abs() < 1e-9); // 1+2+5
    }

    #[test]
    fn test_prim_simple() {
        let g = sample_graph();
        let (mst, total) = prim(&g);
        assert_eq!(mst.len(), 3);
        assert!((total - 8.0).abs() < 1e-9);
    }

    #[test]
    fn test_kruskal_empty() {
        let g = Graph::new(false);
        let (mst, total) = kruskal(&g);
        assert!(mst.is_empty());
        assert!((total - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_kruskal_single_vertex() {
        let mut g = Graph::new(false);
        g.add_vertex(0);
        let (mst, total) = kruskal(&g);
        assert!(mst.is_empty());
        assert!((total - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_kruskal_disconnected() {
        let mut g = Graph::new(false);
        g.add_edge(0, 1, 1.0);
        g.add_edge(2, 3, 2.0);
        let (mst, total) = kruskal(&g);
        assert_eq!(mst.len(), 2); // forest with 2 edges
        assert!((total - 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_prim_disconnected() {
        let mut g = Graph::new(false);
        g.add_edge(0, 1, 1.0);
        g.add_edge(2, 3, 2.0);
        let (mst, total) = prim(&g);
        assert_eq!(mst.len(), 2);
        assert!((total - 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_union_find() {
        let mut uf = UnionFind::new(5);
        assert!(uf.union(0, 1));
        assert!(uf.union(2, 3));
        assert!(!uf.union(0, 1)); // already same
        assert!(uf.union(1, 3));
        assert_eq!(uf.find(0), uf.find(3));
    }
}
