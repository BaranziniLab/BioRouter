#pragma once

/**
 * @file dbg.hpp
 * @brief De Bruijn graph built from k-mers with node/edge structures and contig generation.
 *
 * In a de Bruijn graph for k-mer assembly:
 *   - Nodes are (k-1)-mers.
 *   - Edges connect a (k-1)-mer to another (k-1)-mer if they overlap by (k-2) bases
 *     with a k-mer bridging them. Equivalently, an edge is a k-mer, connecting its
 *     prefix (k-1)-mer to its suffix (k-1)-mer.
 *
 * Contigs are produced by unitig traversal: following linear chains of nodes with
 * in-degree == out-degree == 1.
 */

#include "kmer.hpp"
#include "counter.hpp"
#include <string>
#include <vector>
#include <unordered_map>
#include <unordered_set>
#include <cstddef>
#include <cstdint>
#include <functional>

namespace bkc {

/**
 * @brief A node in the de Bruijn graph, representing a (k-1)-mer.
 */
struct DbgNode {
    uint64_t kmer;       ///< Encoded (k-1)-mer.
    size_t in_degree = 0;
    size_t out_degree = 0;
    size_t coverage = 0;  ///< Sum of edge coverages.
    bool visited = false;
};

/**
 * @brief An edge in the de Bruijn graph, representing a k-mer connecting two nodes.
 */
struct DbgEdge {
    uint64_t kmer;       ///< Encoded k-mer.
    uint64_t src_node;   ///< (k-1)-mer prefix.
    uint64_t dst_node;   ///< (k-1)-mer suffix.
    uint64_t count = 1;  ///< Multiplicity from k-mer counting.
    bool visited = false;
};

/**
 * @brief A contig produced by unitig traversal.
 */
struct Contig {
    std::string sequence;     ///< Assembled sequence.
    size_t length = 0;
    size_t kmer_count = 0;    ///< Number of k-mers spanning the contig.
    double avg_coverage = 0.0;
};

/**
 * @brief Statistics about the de Bruijn graph.
 */
struct DbgStats {
    size_t num_nodes = 0;
    size_t num_edges = 0;
    size_t num_tips = 0;         ///< Nodes with in_deg + out_deg == 1 (dead ends).
    size_t num_bubbles = 0;      ///< Simple bubbles (placeholder).
    size_t num_contigs = 0;
    size_t total_contig_length = 0;
    size_t n50 = 0;              ///< N50 contig length.
    size_t largest_contig = 0;
    double avg_coverage = 0.0;
};

/**
 * @brief De Bruijn graph constructed from a k-mer count table.
 */
class DeBruijnGraph {
public:
    /**
     * @param k  k-mer size used to build the graph.
     */
    explicit DeBruijnGraph(size_t k);

    /**
     * @brief Build the graph from a KmerCounter's results.
     *
     * Only k-mers with count >= min_coverage are included.
     */
    void build(const KmerCounter& counter, uint64_t min_coverage = 1);

    /**
     * @brief Generate contigs via unitig traversal.
     *
     * A unitig is a maximal non-branching path in the graph. Contigs are
     * reconstructed by concatenating the k-mers along each unitig.
     */
    std::vector<Contig> assemble();

    /**
     * @brief Compute graph statistics.
     */
    DbgStats stats() const;

    /**
     * @brief Get all nodes (for inspection / testing).
     */
    const std::unordered_map<uint64_t, DbgNode>& nodes() const { return nodes_; }

    /**
     * @brief Get all edges (for inspection / testing).
     */
    const std::unordered_map<uint64_t, DbgEdge>& edges() const { return edges_; }

    /**
     * @brief Get node by (k-1)-mer key.
     */
    const DbgNode* get_node(uint64_t k1mer) const;

    /**
     * @brief Get edge by k-mer key.
     */
    const DbgEdge* get_edge(uint64_t kmer) const;

private:
    size_t k_;       ///< k-mer size.
    size_t k1_;      ///< (k-1)-mer size.

    std::unordered_map<uint64_t, DbgNode> nodes_;   ///< (k-1)-mer -> node.
    std::unordered_map<uint64_t, DbgEdge> edges_;   ///< k-mer -> edge.

    /**
     * @brief Add a (k-1)-mer node if not present.
     */
    void ensure_node(uint64_t k1mer);

    /**
     * @brief Follow a non-branching path forward from a node, returning the
     *        sequence of edges visited. Stops at branching or already-visited nodes.
     */
    std::vector<uint64_t> follow_unitig(uint64_t start_k1mer);

    /**
     * @brief Reconstruct the DNA string from a unitig (list of k-mers).
     */
    std::string unitig_to_sequence(const std::vector<uint64_t>& unitig_kmers) const;
};

} // namespace bkc
