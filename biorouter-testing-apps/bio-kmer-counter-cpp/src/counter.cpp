/**
 * @file counter.cpp
 * @brief Implementation of hash-map based k-mer counting.
 */

#include "counter.hpp"
#include <algorithm>
#include <cassert>
#include <stdexcept>

namespace bkc {

KmerCounter::KmerCounter(size_t k)
    : k_(k) {
    if (k == 0 || k > MAX_K) {
        throw std::invalid_argument("k must be in [1, " + std::to_string(MAX_K) + "]");
    }
}

void KmerCounter::clear() {
    counts_.clear();
    raw_counts_.clear();
    total_ = 0;
}

void KmerCounter::count(const std::string& seq) {
    if (seq.size() < k_) return;

    // We use a sliding window, resetting on invalid characters.
    size_t run = 0;     // consecutive valid bases in current window
    uint64_t kmer = 0;

    for (size_t i = 0; i < seq.size(); ++i) {
        char c = seq[i];
        if (!is_valid_sequence(std::string(1, c))) {
            // Break the run.
            run = 0;
            kmer = 0;
            continue;
        }

        uint8_t base = encode_base(c);
        kmer = shift_left_append(kmer, k_, base);
        ++run;

        if (run >= k_) {
            // Track the raw (oriented) k-mer for DBG construction.
            raw_counts_[kmer]++;
            // Track the canonical k-mer for strand-independent counting.
            add(canonical(kmer, k_));
        }
    }
}

void KmerCounter::add(uint64_t canonical_kmer) {
    counts_[canonical_kmer]++;
    total_++;
}

uint64_t KmerCounter::get_count(uint64_t canonical_kmer) const {
    auto it = counts_.find(canonical_kmer);
    return (it != counts_.end()) ? it->second : 0;
}

size_t KmerCounter::unique_count() const {
    return counts_.size();
}

uint64_t KmerCounter::total_count() const {
    return total_;
}

size_t KmerCounter::k() const {
    return k_;
}

std::vector<SpectrumEntry> KmerCounter::spectrum() const {
    // Find maximum count.
    uint64_t max_c = 0;
    for (auto& [kmer, c] : counts_) {
        if (c > max_c) max_c = c;
    }

    // Build histogram: freq[c] = number of k-mers with count c.
    std::vector<uint64_t> freq(max_c + 1, 0);
    for (auto& [kmer, c] : counts_) {
        freq[c]++;
    }

    std::vector<SpectrumEntry> result;
    for (uint64_t c = 1; c <= max_c; ++c) {
        if (freq[c] > 0) {
            result.push_back({c, freq[c]});
        }
    }
    return result;
}

uint64_t KmerCounter::max_count() const {
    uint64_t mx = 0;
    for (auto& [kmer, c] : counts_) {
        if (c > mx) mx = c;
    }
    return mx;
}

} // namespace bkc
