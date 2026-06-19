/**
 * @file benchmark_kmer.cpp
 * @brief Performance benchmark for k-mer counting.
 */

#include "kmer.hpp"
#include "counter.hpp"
#include "dbg.hpp"
#include "io.hpp"
#include <iostream>
#include <chrono>
#include <string>
#include <random>
#include <iomanip>
#include <vector>

using namespace bkc;

/// Generate a random DNA sequence of given length.
static std::string random_sequence(size_t length, unsigned seed = 42) {
    std::mt19937 gen(seed);
    std::uniform_int_distribution<int> dist(0, 3);
    static const char bases[4] = {'A', 'C', 'G', 'T'};
    std::string seq(length, 'A');
    for (auto& c : seq) {
        c = bases[dist(gen)];
    }
    return seq;
}

/// Benchmark: encode + decode round-trip.
static void bench_encode_decode(size_t num_ops) {
    auto start = std::chrono::high_resolution_clock::now();

    std::string seq = "ACGTACGTACGTACGT";
    for (size_t i = 0; i < num_ops; ++i) {
        volatile uint64_t kmer = encode_kmer(seq);
        volatile std::string dec = decode_kmer(kmer, seq.size());
        (void)dec;
    }

    auto end = std::chrono::high_resolution_clock::now();
    auto ms = std::chrono::duration_cast<std::chrono::milliseconds>(end - start).count();
    std::cout << "  Encode+decode (" << num_ops << " ops):   "
              << ms << " ms\n";
}

/// Benchmark: canonical k-mer computation.
static void bench_canonical(size_t num_ops) {
    auto start = std::chrono::high_resolution_clock::now();

    std::string seq = "ACGTACGTACGTACGT";
    size_t k = seq.size();
    uint64_t kmer = encode_kmer(seq);
    for (size_t i = 0; i < num_ops; ++i) {
        volatile uint64_t c = canonical(kmer, k);
        (void)c;
    }

    auto end = std::chrono::high_resolution_clock::now();
    auto ms = std::chrono::duration_cast<std::chrono::milliseconds>(end - start).count();
    std::cout << "  Canonical (" << num_ops << " ops):       "
              << ms << " ms\n";
}

/// Benchmark: k-mer counting on a synthetic sequence.
static void bench_kmer_counting(size_t seq_len, size_t k) {
    auto seq = random_sequence(seq_len);

    auto start = std::chrono::high_resolution_clock::now();

    KmerCounter counter(k);
    counter.count(seq);

    auto end = std::chrono::high_resolution_clock::now();
    auto ms = std::chrono::duration_cast<std::chrono::milliseconds>(end - start).count();
    double mbases_per_sec = (seq_len / 1e6) / (ms / 1e3);

    std::cout << "  Count k=" << k << " on " << seq_len / 1000 << "Kbp:  "
              << ms << " ms (" << std::fixed << std::setprecision(2)
              << mbases_per_sec << " Mbp/s)\n"
              << "    Unique: " << counter.unique_count()
              << ", Total: " << counter.total_count() << "\n";
}

/// Benchmark: de Bruijn graph build + assemble.
static void bench_dbg_assembly(size_t seq_len, size_t k) {
    auto seq = random_sequence(seq_len);

    auto start_total = std::chrono::high_resolution_clock::now();

    KmerCounter counter(k);
    counter.count(seq);

    DeBruijnGraph graph(k);
    graph.build(counter);

    auto start_assemble = std::chrono::high_resolution_clock::now();

    auto contigs = graph.assemble();

    auto end = std::chrono::high_resolution_clock::now();

    auto ms_build = std::chrono::duration_cast<std::chrono::milliseconds>(
        start_assemble - start_total).count();
    auto ms_assemble = std::chrono::duration_cast<std::chrono::milliseconds>(
        end - start_assemble).count();
    auto ms_total = std::chrono::duration_cast<std::chrono::milliseconds>(
        end - start_total).count();

    std::cout << "  DBG k=" << k << " on " << seq_len / 1000 << "Kbp:\n"
              << "    Build:    " << ms_build << " ms\n"
              << "    Assemble: " << ms_assemble << " ms\n"
              << "    Total:    " << ms_total << " ms\n"
              << "    Contigs:  " << contigs.size() << "\n";

    size_t total_len = 0;
    for (auto& c : contigs) total_len += c.length;
    std::cout << "    Total contig length: " << total_len << " bp\n";
}

int main() {
    std::cout << "========================================\n";
    std::cout << "  bio-kmer-counter C++ Benchmark\n";
    std::cout << "========================================\n\n";

    std::cout << "--- Micro-benchmarks ---\n";
    bench_encode_decode(10000000);
    bench_canonical(10000000);

    std::cout << "\n--- K-mer counting ---\n";
    bench_kmer_counting(100000, 21);
    bench_kmer_counting(500000, 21);
    bench_kmer_counting(1000000, 21);

    std::cout << "\n--- K-mer counting (varying k) ---\n";
    bench_kmer_counting(500000, 11);
    bench_kmer_counting(500000, 21);
    bench_kmer_counting(500000, 31);

    std::cout << "\n--- De Bruijn graph assembly ---\n";
    bench_dbg_assembly(100000, 21);
    bench_dbg_assembly(500000, 21);

    std::cout << "\n========================================\n";
    std::cout << "  Benchmark complete.\n";
    std::cout << "========================================\n";

    return 0;
}
