/**
 * @file kmer.cpp
 * @brief Implementation of 2-bit nucleotide encoding and k-mer operations.
 */

#include "kmer.hpp"
#include <algorithm>
#include <sstream>
#include <unordered_set>
#include <cmath>
#include <cstring>

namespace bkc {

// --- Base encoding / decoding ---

uint8_t encode_base(char base) {
    switch (base) {
        case 'A': case 'a': return 0b00;
        case 'C': case 'c': return 0b01;
        case 'G': case 'g': return 0b10;
        case 'T': case 't': return 0b11;
        default:
            throw std::invalid_argument(
                std::string("Invalid nucleotide character: '") + base + "'");
    }
}

char decode_base(uint8_t code) {
    static constexpr char table[4] = {'A', 'C', 'G', 'T'};
    if (code > 3) {
        throw std::invalid_argument("Invalid 2-bit code: " + std::to_string(code));
    }
    return table[code];
}

// --- K-mer encoding / decoding ---

uint64_t encode_kmer(const std::string& seq) {
    if (seq.size() > MAX_K) {
        throw std::invalid_argument(
            "Sequence length " + std::to_string(seq.size()) +
            " exceeds MAX_K (" + std::to_string(MAX_K) + ")");
    }
    uint64_t kmer = 0;
    for (char c : seq) {
        kmer = (kmer << 2) | encode_base(c);
    }
    return kmer;
}

std::string decode_kmer(uint64_t kmer, size_t k) {
    std::string result(k, 'A');
    // We work from right to left
    for (size_t i = k; i > 0; --i) {
        result[i - 1] = decode_base(static_cast<uint8_t>(kmer & 0b11));
        kmer >>= 2;
    }
    return result;
}

uint64_t reverse_complement(uint64_t kmer, size_t k) {
    // Reverse complement: complement each base, then reverse.
    // Complement: swap 00<->11, 01<->10 => XOR with 0b11 per base.
    // We'll reverse by extracting from LSB and building new value.
    uint64_t rc = 0;
    for (size_t i = 0; i < k; ++i) {
        uint8_t base = static_cast<uint8_t>(kmer & 0b11);
        uint8_t comp = base ^ 0b11;  // complement
        rc = (rc << 2) | comp;
        kmer >>= 2;
    }
    return rc;
}

uint64_t canonical(uint64_t kmer, size_t k) {
    uint64_t rc = reverse_complement(kmer, k);
    return (kmer <= rc) ? kmer : rc;
}

uint64_t shift_left_append(uint64_t kmer, size_t k, uint8_t new_base) {
    // Mask out the leftmost 2 bits, shift left, OR in new base at LSB.
    uint64_t mask = (~uint64_t(0)) >> (64 - 2 * k);  // mask for k bases
    return ((kmer << 2) | new_base) & mask;
}

uint8_t leftmost_base(uint64_t kmer, size_t k) {
    return static_cast<uint8_t>((kmer >> (2 * (k - 1))) & 0b11);
}

uint8_t rightmost_base(uint64_t kmer) {
    return static_cast<uint8_t>(kmer & 0b11);
}

uint64_t prefix(uint64_t kmer, size_t k) {
    // Drop rightmost 2 bits.
    return kmer >> 2;
}

uint64_t suffix(uint64_t kmer, size_t k) {
    // Drop leftmost 2 bits.
    uint64_t mask = (~uint64_t(0)) >> (64 - 2 * (k - 1));
    return kmer & mask;
}

bool is_valid_sequence(const std::string& seq) {
    for (char c : seq) {
        switch (c) {
            case 'A': case 'a': case 'C': case 'c':
            case 'G': case 'g': case 'T': case 't':
                continue;
            default:
                return false;
        }
    }
    return true;
}

double gc_content(const std::string& seq) {
    if (seq.empty()) return 0.0;
    size_t gc = 0;
    for (char c : seq) {
        switch (c) {
            case 'G': case 'g': case 'C': case 'c':
                ++gc;
                break;
            default:
                break;
        }
    }
    return static_cast<double>(gc) / seq.size();
}

double sequence_complexity(const std::string& seq, size_t k) {
    if (seq.size() < k) return 1.0;

    size_t total = seq.size() - k + 1;
    std::unordered_set<uint64_t> unique;

    // Encode first k-mer
    std::string first = seq.substr(0, k);
    if (!is_valid_sequence(first)) return 0.0;

    uint64_t kmer = encode_kmer(first);
    unique.insert(canonical(kmer, k));

    // Slide window
    for (size_t i = 1; i <= seq.size() - k; ++i) {
        char new_char = seq[i + k - 1];
        if (!is_valid_sequence(std::string(1, new_char))) return 0.0;
        uint8_t base = encode_base(new_char);
        kmer = shift_left_append(kmer, k, base);
        unique.insert(canonical(kmer, k));
    }

    // Max possible unique k-mers for 4-base alphabet is 4^k.
    // Cap the denominator to avoid overflow for large k.
    double max_kmers = 1.0;
    for (size_t i = 0; i < k; ++i) max_kmers *= 4.0;
    double ratio = static_cast<double>(unique.size()) / std::min(max_kmers, static_cast<double>(total));
    return std::min(ratio, 1.0);
}

} // namespace bkc
