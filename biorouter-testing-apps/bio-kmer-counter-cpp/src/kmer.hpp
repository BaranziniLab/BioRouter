#pragma once

/**
 * @file kmer.hpp
 * @brief 2-bit nucleotide encoding, canonical k-mer operations.
 *
 * Encoding: A=0b00, C=0b01, G=0b10, T=0b11
 * A k-mer of length k is stored in 2*k bits, packed into a uint64_t.
 * A canonical k-mer is the lexicographically smaller of a k-mer and its
 * reverse complement, ensuring strand-independent representation.
 */

#include <cstdint>
#include <cstddef>
#include <string>
#include <stdexcept>
#include <functional>
#include <array>
#include <optional>

namespace bkc {

/// Maximum k supported (64 bits / 2 bits per base = 32).
inline constexpr size_t MAX_K = 32;

/**
 * @brief 2-bit encoding of a single nucleotide.
 *
 * Encodes A, C, G, T into 2-bit values.
 * Invalid characters throw std::invalid_argument.
 */
uint8_t encode_base(char base);

/**
 * @brief Decode a 2-bit value back to a nucleotide character.
 */
char decode_base(uint8_t code);

/**
 * @brief Encode a DNA string into a packed 64-bit k-mer.
 *
 * @param seq  DNA sequence (A/C/G/T). Length must be <= MAX_K.
 * @return     Packed k-mer (bit-packed, left-aligned in uint64_t).
 */
uint64_t encode_kmer(const std::string& seq);

/**
 * @brief Decode a packed k-mer back into a DNA string.
 *
 * @param kmer  Packed k-mer value.
 * @param k     Length of the k-mer.
 * @return      Decoded DNA string of length k.
 */
std::string decode_kmer(uint64_t kmer, size_t k);

/**
 * @brief Compute the reverse complement of a packed k-mer.
 */
uint64_t reverse_complement(uint64_t kmer, size_t k);

/**
 * @brief Return the canonical (lexicographically smaller) form of a k-mer.
 *
 * Compares a k-mer with its reverse complement and returns the smaller one.
 */
uint64_t canonical(uint64_t kmer, size_t k);

/**
 * @brief Shift a k-mer left by one base and append a new base.
 *
 * Used for sliding-window k-mer extraction. Drops the leftmost base.
 */
uint64_t shift_left_append(uint64_t kmer, size_t k, uint8_t new_base);

/**
 * @brief Get the leftmost (5') base of a packed k-mer.
 */
uint8_t leftmost_base(uint64_t kmer, size_t k);

/**
 * @brief Get the rightmost (3') base of a packed k-mer.
 */
uint8_t rightmost_base(uint64_t kmer);

/**
 * @brief Get the (k-1)-mer prefix (drop rightmost base).
 */
uint64_t prefix(uint64_t kmer, size_t k);

/**
 * @brief Get the (k-1)-mer suffix (drop leftmost base).
 */
uint64_t suffix(uint64_t kmer, size_t k);

/**
 * @brief Validate that a string contains only valid nucleotide characters.
 */
bool is_valid_sequence(const std::string& seq);

/**
 * @brief Struct holding k-mer statistics.
 */
struct KmerStats {
    size_t total_kmers = 0;      ///< Total k-mers extracted (including duplicates).
    size_t unique_kmers = 0;     ///< Unique canonical k-mers.
    double gc_content = 0.0;     ///< GC fraction of the input sequence.
    size_t invalid_bases = 0;    ///< Number of non-ACGT characters encountered.
};

/**
 * @brief Compute GC content of a string.
 */
double gc_content(const std::string& seq);

/**
 * @brief Compute sequence complexity (k-mer diversity ratio).
 *
 * @param seq     Input sequence.
 * @param k       k-mer size to measure (should be small, e.g., 3).
 * @return        Ratio of unique k-mers to total possible k-mers (capped at 1.0).
 */
double sequence_complexity(const std::string& seq, size_t k = 3);

} // namespace bkc
