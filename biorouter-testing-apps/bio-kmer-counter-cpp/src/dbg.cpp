/**
 * @file dbg.cpp
 * @brief Implementation of de Bruijn graph construction and unitig traversal.
 */

#include "dbg.hpp"
#include <algorithm>
#include <numeric>
#include <cassert>
#include <stdexcept>
#include <iostream>

namespace bkc {

DeBruijnGraph::DeBruijnGraph(size_t k)
    : k_(k), k1_(k > 0 ? k - 1 : 0) {
    if (k < 2) {
        throw std::invalid_argument("k must be >= 2 for de Bruijn graph construction");
    }
}

void DeBruijnGraph::ensure_node(uint64_t k1mer) {
    if (nodes_.find(k1mer) == nodes_.end()) {
        nodes_[k1mer] = DbgNode{k1mer, 0, 0, 0, false};
    }
}

void DeBruijnGraph::build(const KmerCounter& counter, uint64_t min_coverage) {
    nodes_.clear();
    edges_.clear();

    size_t k = counter.k();
    if (k != k_) {
        throw std::invalid_argument("Counter k (" + std::to_string(k) +
            ") does not match graph k (" + std::to_string(k_) + ")");
    }

    // Build from raw (oriented) k-mers, not canonical ones.
    // This preserves the correct graph topology for assembly.
    for (auto& [kmer, count] : counter.raw_counts()) {
        if (count < min_coverage) continue;

        uint64_t pfx = prefix(kmer, k_);
        uint64_t sfx = suffix(kmer, k_);

        ensure_node(pfx);
        ensure_node(sfx);

        edges_[kmer] = DbgEdge{kmer, pfx, sfx, count, false};

        nodes_[pfx].out_degree++;
        nodes_[pfx].coverage += count;
        nodes_[sfx].in_degree++;
        nodes_[sfx].coverage += count;
    }
}

std::vector<uint64_t> DeBruijnGraph::follow_unitig(uint64_t start_k1mer) {
    std::vector<uint64_t> unitig;
    unitig.push_back(start_k1mer);

    // Walk forward.
    uint64_t current = start_k1mer;
    while (true) {
        auto node_it = nodes_.find(current);
        if (node_it == nodes_.end()) break;
        DbgNode& node = node_it->second;

        // A unitig continues only if out_degree == 1 and we haven't visited.
        if (node.out_degree != 1 || node.visited) break;
        node.visited = true;

        // Find the single outgoing edge: iterate edges to find one whose src matches.
        bool found = false;
        for (auto& [ek, edge] : edges_) {
            if (edge.src_node == current && !edge.visited) {
                edge.visited = true;
                unitig.push_back(edge.dst_node);
                current = edge.dst_node;
                found = true;
                break;
            }
        }
        if (!found) break;
    }

    return unitig;
}

std::string DeBruijnGraph::unitig_to_sequence(const std::vector<uint64_t>& unitig_kmers) const {
    if (unitig_kmers.empty()) return "";

    // First (k-1)-mer contributes all its bases.
    std::string seq = decode_kmer(unitig_kmers[0], k1_);

    // Each subsequent (k-1)-mer contributes its last base.
    for (size_t i = 1; i < unitig_kmers.size(); ++i) {
        seq += decode_base(rightmost_base(unitig_kmers[i]));
    }

    return seq;
}

std::vector<Contig> DeBruijnGraph::assemble() {
    // Reset visited flags.
    for (auto& [k, node] : nodes_) {
        node.visited = false;
    }
    for (auto& [k, edge] : edges_) {
        edge.visited = false;
    }

    std::vector<Contig> contigs;

    // Phase 1: Walk from tip nodes (in=0, out>=1) — these are sequence starts.
    for (auto& [k1mer, node] : nodes_) {
        if (node.visited) continue;

        bool is_tip_start = (node.in_degree == 0 && node.out_degree >= 1);
        if (!is_tip_start) continue;

        auto unitig = follow_unitig(k1mer);
        if (unitig.size() < 2) {
            continue;
        }

        Contig c;
        c.sequence = unitig_to_sequence(unitig);
        c.length = c.sequence.size();
        c.kmer_count = unitig.size() - 1;

        double sum_cov = 0.0;
        for (auto nkey : unitig) {
            auto nit = nodes_.find(nkey);
            if (nit != nodes_.end()) sum_cov += nit->second.coverage;
        }
        c.avg_coverage = sum_cov / unitig.size();
        contigs.push_back(std::move(c));
    }

    // Phase 2: Walk from remaining unvisited linear nodes (in=1, out=1).
    // These form internal segments not connected to tips (e.g., in cycles
    // or disconnected components).
    for (auto& [k1mer, node] : nodes_) {
        if (node.visited) continue;
        if (node.in_degree != 1 || node.out_degree != 1) continue;

        auto unitig = follow_unitig(k1mer);
        if (unitig.size() < 2) continue;

        Contig c;
        c.sequence = unitig_to_sequence(unitig);
        c.length = c.sequence.size();
        c.kmer_count = unitig.size() - 1;

        double sum_cov = 0.0;
        for (auto nkey : unitig) {
            auto nit = nodes_.find(nkey);
            if (nit != nodes_.end()) sum_cov += nit->second.coverage;
        }
        c.avg_coverage = sum_cov / unitig.size();
        contigs.push_back(std::move(c));
    }

    // Phase 3: Handle cycles — trace from unvisited edges.
    for (auto& [kmer, edge] : edges_) {
        if (edge.visited) continue;

        std::vector<uint64_t> cycle;
        cycle.push_back(edge.src_node);
        cycle.push_back(edge.dst_node);
        edge.visited = true;

        uint64_t cur = edge.dst_node;
        while (true) {
            auto node_it = nodes_.find(cur);
            if (node_it == nodes_.end()) break;
            DbgNode& nd = node_it->second;
            if (nd.out_degree != 1) break;

            bool found_edge = false;
            for (auto& [ek, e] : edges_) {
                if (e.src_node == cur && !e.visited) {
                    e.visited = true;
                    cycle.push_back(e.dst_node);
                    cur = e.dst_node;
                    found_edge = true;
                    break;
                }
            }
            if (!found_edge) break;
            if (cur == cycle[0]) break;
        }

        if (cycle.size() >= 3) {
            bool is_cycle = (cur == cycle[0]);

            Contig c;
            c.sequence = unitig_to_sequence(cycle);
            c.length = c.sequence.size();
            c.kmer_count = cycle.size() - 1;
            if (is_cycle) {
                c.sequence += decode_base(rightmost_base(cycle[0]));
                c.length = c.sequence.size();
                c.kmer_count = cycle.size();
            }

            double sum_cov = 0.0;
            for (auto nkey : cycle) {
                auto nit = nodes_.find(nkey);
                if (nit != nodes_.end()) sum_cov += nit->second.coverage;
            }
            c.avg_coverage = sum_cov / cycle.size();
            contigs.push_back(std::move(c));
        }
    }

    return contigs;
}

DbgStats DeBruijnGraph::stats() const {
    DbgStats s;
    s.num_nodes = nodes_.size();
    s.num_edges = edges_.size();

    for (auto& [k, node] : nodes_) {
        if (node.in_degree + node.out_degree == 1) s.num_tips++;
    }

    // Compute contig-related stats from edges.
    s.avg_coverage = 0.0;
    uint64_t total_cov = 0;
    for (auto& [k, edge] : edges_) {
        total_cov += edge.count;
    }
    if (!edges_.empty()) {
        s.avg_coverage = static_cast<double>(total_cov) / edges_.size();
    }

    // N50 and total length — approximate from edge counts and k.
    s.num_contigs = 0;
    s.total_contig_length = 0;

    // Simple estimate: each edge contributes ~1 new base beyond (k-1).
    // For a proper N50 we'd need to run assemble() but that's a side-effect.
    // Instead, we compute from connected component sizes.
    // We'll use a simpler approach: count edges as proxy for contig length.
    // Proper stats come from assemble().

    // To compute N50 without assembling, we can walk unitigs from the graph.
    // But let's keep stats() lightweight. We just report graph-level stats.
    s.num_contigs = s.num_edges; // placeholder — use assemble for real
    s.total_contig_length = s.num_edges + s.num_nodes; // rough estimate
    s.largest_contig = 0;

    return s;
}

const DbgNode* DeBruijnGraph::get_node(uint64_t k1mer) const {
    auto it = nodes_.find(k1mer);
    return (it != nodes_.end()) ? &it->second : nullptr;
}

const DbgEdge* DeBruijnGraph::get_edge(uint64_t kmer) const {
    auto it = edges_.find(kmer);
    return (it != edges_.end()) ? &it->second : nullptr;
}

} // namespace bkc
