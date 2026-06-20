#pragma once

/**
 * @file counter.hpp
 * @brief Hash-map based k-mer counter with configurable k.
 *
 * Extracts canonical k-mers from a sequence and counts their occurrences.
 * Produces a k-mer frequency spectrum (histogram).
 */

#include "kmer.hpp"
#include <string>
#include <vector>
#include <unordered_map>
#include <cstddef>
#include <cstdint>
#include <limits>

namespace bkc {

/**
 * @brief A k-mer frequency histogram entry: (count, number_of_kmers_with_that_count).
 */
struct SpectrumEntry {
    uint64_t count;      ///< Occurrence count of a k-mer class.
    uint64_t frequency;  ///< Number of distinct k-mers with this count.
};

/**
 * @brief KmerCounter accumulates canonical k-mer counts.
 */
class KmerCounter {
public:
    /**
     * @param k  k-mer size (1..MAX_K).
     */
    explicit KmerCounter(size_t k);

    /**
     * @brief Reset all counts to zero.
     */
    void clear();

    /**
     * @brief Feed a sequence string; extract and count all canonical k-mers.
     *
     * Characters outside {A,C,G,T} are skipped (they break k-mer boundaries).
     */
    void count(const std::string& seq);

    /**
     * @brief Count a single pre-extracted k-mer.
     */
    void add(uint64_t canonical_kmer);

    /**
     * @brief Return the raw count for a specific canonical k-mer.
     */
    uint64_t get_count(uint64_t canonical_kmer) const;

    /**
     * @brief Return the number of distinct canonical k-mers observed.
     */
    size_t unique_count() const;

    /**
     * @brief Return the total number of k-mers counted (including duplicates).
     */
    uint64_t total_count() const;

    /**
     * @brief Return the configured k.
     */
    size_t k() const;

    /**
     * @brief Compute the k-mer frequency spectrum (histogram).
     *
     * Returns a vector of SpectrumEntry sorted by count ascending.
     * Entry (c, n) means "n distinct k-mers appear exactly c times".
     */
    std::vector<SpectrumEntry> spectrum() const;

    /**
     * @brief Return the count of the most abundant k-mer.
     */
    uint64_t max_count() const;

    /**
     * @brief Return a reference to the canonical count map (for iteration).
     */
    const std::unordered_map<uint64_t, uint64_t>& counts() const { return counts_; }

    /**
     * @brief Return a reference to the raw (oriented) k-mer count map.
     *
     * This tracks each k-mer in its original orientation, which is needed
     * for de Bruijn graph construction and assembly. The canonical map
     * collapses both strands; the raw map preserves orientation.
     */
    const std::unordered_map<uint64_t, uint64_t>& raw_counts() const { return raw_counts_; }

private:
    size_t k_;
    std::unordered_map<uint64_t, uint64_t> counts_;      ///< Canonical k-mer counts.
    std::unordered_map<uint64_t, uint64_t> raw_counts_;  ///< Oriented k-mer counts.
    uint64_t total_ = 0;
};

} // namespace bkc
