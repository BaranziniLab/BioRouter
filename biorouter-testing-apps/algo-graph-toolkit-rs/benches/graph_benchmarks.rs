//! Criterion benchmarks for the heavier algorithms.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use algo_graph_toolkit_rs::components::{kosaraju_scc, tarjan_scc};
use algo_graph_toolkit_rs::flow::edmonds_karp;
use algo_graph_toolkit_rs::graph::Graph;
use algo_graph_toolkit_rs::mst::{kruskal, prim};
use algo_graph_toolkit_rs::shortest_path::{bellman_ford, dijkstra, floyd_warshall};
use algo_graph_toolkit_rs::toposort::{topological_sort, topological_sort_kahn};
use algo_graph_toolkit_rs::traversal::{bfs, dfs};

fn build_dense_digraph(n: usize) -> Graph {
    let mut g = Graph::new(true);
    for i in 0..n {
        for j in 0..n {
            if i != j {
                g.add_edge(i, j, (i + j) as f64 + 1.0);
            }
        }
    }
    g
}

fn build_sparse_undirected(n: usize) -> Graph {
    let mut g = Graph::new(false);
    for i in 0..n - 1 {
        g.add_edge(i, i + 1, (i + 1) as f64);
        if i + 2 < n {
            g.add_edge(i, i + 2, (i + 2) as f64 * 0.5);
        }
    }
    g
}

fn build_grid_graph(n: usize) -> Graph {
    let mut g = Graph::new(false);
    for i in 0..n {
        for j in 0..n {
            let v = i * n + j;
            if j + 1 < n {
                g.add_edge(v, v + 1, 1.0);
            }
            if i + 1 < n {
                g.add_edge(v, v + n, 1.0);
            }
        }
    }
    g
}

fn build_flow_network(n: usize) -> Graph {
    let mut g = Graph::new(true);
    for i in 0..n {
        for j in 0..n {
            if i != j {
                g.add_edge(i, j, ((i * 7 + j * 13) % 50 + 1) as f64);
            }
        }
    }
    g
}

fn bench_bfs(c: &mut Criterion) {
    let g = build_grid_graph(100);
    c.bench_function("bfs_100x100_grid", |b| {
        b.iter(|| bfs(black_box(&g), black_box(0)))
    });
}

fn bench_dfs(c: &mut Criterion) {
    let g = build_grid_graph(100);
    c.bench_function("dfs_100x100_grid", |b| {
        b.iter(|| dfs(black_box(&g), black_box(0)))
    });
}

fn bench_toposort(c: &mut Criterion) {
    let g = build_dense_digraph(200);
    c.bench_function("toposort_dense_200", |b| {
        b.iter(|| topological_sort(black_box(&g)))
    });
}

fn bench_kahn(c: &mut Criterion) {
    let g = build_dense_digraph(200);
    c.bench_function("kahn_dense_200", |b| {
        b.iter(|| topological_sort_kahn(black_box(&g)))
    });
}

fn bench_tarjan(c: &mut Criterion) {
    let g = build_dense_digraph(100);
    c.bench_function("tarjan_scc_dense_100", |b| {
        b.iter(|| tarjan_scc(black_box(&g)))
    });
}

fn bench_kosaraju(c: &mut Criterion) {
    let g = build_dense_digraph(100);
    c.bench_function("kosaraju_scc_dense_100", |b| {
        b.iter(|| kosaraju_scc(black_box(&g)))
    });
}

fn bench_dijkstra(c: &mut Criterion) {
    let g = build_sparse_undirected(1000);
    c.bench_function("dijkstra_sparse_1000", |b| {
        b.iter(|| dijkstra(black_box(&g), black_box(0)))
    });
}

fn bench_bellman_ford(c: &mut Criterion) {
    let g = build_sparse_undirected(500);
    c.bench_function("bellman_ford_sparse_500", |b| {
        b.iter(|| bellman_ford(black_box(&g), black_box(0)))
    });
}

fn bench_floyd_warshall(c: &mut Criterion) {
    let g = build_sparse_undirected(200);
    c.bench_function("floyd_warshall_sparse_200", |b| {
        b.iter(|| floyd_warshall(black_box(&g)))
    });
}

fn bench_kruskal(c: &mut Criterion) {
    let g = build_sparse_undirected(1000);
    c.bench_function("kruskal_sparse_1000", |b| {
        b.iter(|| kruskal(black_box(&g)))
    });
}

fn bench_prim(c: &mut Criterion) {
    let g = build_sparse_undirected(1000);
    c.bench_function("prim_sparse_1000", |b| {
        b.iter(|| prim(black_box(&g)))
    });
}

fn bench_max_flow(c: &mut Criterion) {
    let g = build_flow_network(50);
    c.bench_function("max_flow_dense_50", |b| {
        b.iter(|| edmonds_karp(black_box(&g), black_box(0), black_box(49)))
    });
}

criterion_group!(
    benches,
    bench_bfs,
    bench_dfs,
    bench_toposort,
    bench_kahn,
    bench_tarjan,
    bench_kosaraju,
    bench_dijkstra,
    bench_bellman_ford,
    bench_floyd_warshall,
    bench_kruskal,
    bench_prim,
    bench_max_flow,
);
criterion_main!(benches);
