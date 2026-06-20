/**
 * @file test_kmer.cpp
 * @brief Tests for 2-bit nucleotide encoding and k-mer operations.
 */

#include "test_framework.hpp"
#include "kmer.hpp"
#include <cstdint>

using namespace bkc;

// ========== Base encoding tests ==========

TEST(encode_base_A) {
    ASSERT_EQ(encode_base('A'), 0b00u);
}

TEST(encode_base_C) {
    ASSERT_EQ(encode_base('C'), 0b01u);
}

TEST(encode_base_G) {
    ASSERT_EQ(encode_base('G'), 0b10u);
}

TEST(encode_base_T) {
    ASSERT_EQ(encode_base('T'), 0b11u);
}

TEST(encode_base_lowercase) {
    ASSERT_EQ(encode_base('a'), 0b00u);
    ASSERT_EQ(encode_base('c'), 0b01u);
    ASSERT_EQ(encode_base('g'), 0b10u);
    ASSERT_EQ(encode_base('t'), 0b11u);
}

TEST(encode_base_invalid) {
    ASSERT_THROWS(encode_base('N'), std::invalid_argument);
    ASSERT_THROWS(encode_base('X'), std::invalid_argument);
    ASSERT_THROWS(encode_base('-'), std::invalid_argument);
}

TEST(decode_base_roundtrip) {
    for (uint8_t i = 0; i < 4; ++i) {
        char decoded = decode_base(i);
        uint8_t re_encoded = encode_base(decoded);
        ASSERT_EQ(re_encoded, i);
    }
}

TEST(decode_base_invalid) {
    ASSERT_THROWS(decode_base(4), std::invalid_argument);
    ASSERT_THROWS(decode_base(255), std::invalid_argument);
}

// ========== K-mer encoding tests ==========

TEST(encode_single_base) {
    ASSERT_EQ(encode_kmer("A"), 0b00u);
    ASSERT_EQ(encode_kmer("C"), 0b01u);
    ASSERT_EQ(encode_kmer("G"), 0b10u);
    ASSERT_EQ(encode_kmer("T"), 0b11u);
}

TEST(encode_two_bases) {
    // "AC" = A(00) shifted left, then C(01): 0001
    ASSERT_EQ(encode_kmer("AC"), 0b0001u);
    // "TG" = T(11) shifted left, then G(10): 1110
    ASSERT_EQ(encode_kmer("TG"), 0b1110u);
}

TEST(encode_three_bases) {
    // "ACG" = A(00) << 4 | C(01) << 2 | G(10) = 00 01 10
    ASSERT_EQ(encode_kmer("ACG"), 0b000110u);
}

TEST(encode_max_k) {
    std::string seq(MAX_K, 'A');
    uint64_t result = encode_kmer(seq);
    ASSERT_EQ(result, 0u);
}

TEST(encode_overflow_throws) {
    std::string too_long(MAX_K + 1, 'A');
    ASSERT_THROWS(encode_kmer(too_long), std::invalid_argument);
}

// ========== Round-trip tests ==========

TEST(decode_single_base) {
    ASSERT_EQ(decode_kmer(0b00, 1), "A");
    ASSERT_EQ(decode_kmer(0b01, 1), "C");
    ASSERT_EQ(decode_kmer(0b10, 1), "G");
    ASSERT_EQ(decode_kmer(0b11, 1), "T");
}

TEST(encode_decode_roundtrip) {
    std::string original = "ACGTACGT";
    uint64_t encoded = encode_kmer(original);
    std::string decoded = decode_kmer(encoded, original.size());
    ASSERT_EQ(decoded, original);
}

TEST(encode_decode_roundtrip_k5) {
    std::string original = "GCGAT";
    uint64_t encoded = encode_kmer(original);
    std::string decoded = decode_kmer(encoded, original.size());
    ASSERT_EQ(decoded, original);
}

TEST(encode_decode_all_A) {
    std::string seq(10, 'A');
    uint64_t enc = encode_kmer(seq);
    ASSERT_EQ(enc, 0u);
    std::string dec = decode_kmer(enc, 10);
    ASSERT_EQ(dec, seq);
}

TEST(encode_decode_all_T) {
    std::string seq(8, 'T');
    uint64_t enc = encode_kmer(seq);
    std::string dec = decode_kmer(enc, 8);
    ASSERT_EQ(dec, seq);
}

// ========== Reverse complement tests ==========

TEST(reverse_complement_single_A) {
    // A (00) -> complement T (11), reversed = T
    uint64_t rc = reverse_complement(encode_kmer("A"), 1);
    ASSERT_EQ(decode_kmer(rc, 1), "T");
}

TEST(reverse_complement_single_C) {
    // C (01) -> complement G (10), reversed = G
    uint64_t rc = reverse_complement(encode_kmer("C"), 1);
    ASSERT_EQ(decode_kmer(rc, 1), "G");
}

TEST(reverse_complement_AC) {
    // "AC" -> complement "TG" -> reverse "GT"
    uint64_t kmer = encode_kmer("AC");
    uint64_t rc = reverse_complement(kmer, 2);
    ASSERT_EQ(decode_kmer(rc, 2), "GT");
}

TEST(reverse_complement_palindrome) {
    // "AT" -> complement "TA" -> reverse "AT"  (palindrome!)
    uint64_t kmer = encode_kmer("AT");
    uint64_t rc = reverse_complement(kmer, 2);
    ASSERT_EQ(rc, kmer);
    ASSERT_EQ(decode_kmer(rc, 2), "AT");
}

TEST(reverse_complement_is_own_reverse) {
    // Applying RC twice should return the original.
    std::string seq = "ACGTACGT";
    uint64_t kmer = encode_kmer(seq);
    uint64_t rc = reverse_complement(kmer, seq.size());
    uint64_t rc2 = reverse_complement(rc, seq.size());
    ASSERT_EQ(rc2, kmer);
}

// ========== Canonical k-mer tests ==========

TEST(canonical_uses_smaller) {
    // "AC" (0001) vs reverse complement "GT" (1011)
    // "AC" < "GT", so canonical should be "AC"
    uint64_t kmer = encode_kmer("AC");
    uint64_t canon = canonical(kmer, 2);
    ASSERT_EQ(canon, kmer);
}

TEST(canonical_palindrome) {
    // If k-mer equals its RC, canonical should be itself.
    std::string seq = "AT";  // RC is also "AT"
    uint64_t kmer = encode_kmer(seq);
    uint64_t canon = canonical(kmer, seq.size());
    ASSERT_EQ(canon, kmer);
}

TEST(canonical_strand_independent) {
    // The canonical form should be the same regardless of input strand.
    std::string seq = "ACGTACGT";
    uint64_t kmer = encode_kmer(seq);
    uint64_t kmer_rc = reverse_complement(kmer, seq.size());

    uint64_t canon1 = canonical(kmer, seq.size());
    uint64_t canon2 = canonical(kmer_rc, seq.size());
    ASSERT_EQ(canon1, canon2);
}

TEST(canonical_k3_consistent) {
    // For all k=3 k-mers, canonical(kmer) == canonical(rc(kmer)).
    std::string bases = "ACGT";
    for (char b1 : bases) {
        for (char b2 : bases) {
            for (char b3 : bases) {
                std::string seq = std::string(1, b1) + b2 + b3;
                uint64_t kmer = encode_kmer(seq);
                uint64_t rc = reverse_complement(kmer, 3);
                uint64_t c1 = canonical(kmer, 3);
                uint64_t c2 = canonical(rc, 3);
                ASSERT_EQ(c1, c2);
            }
        }
    }
}

// ========== Shift / prefix / suffix tests ==========

TEST(shift_left_append) {
    uint64_t kmer = encode_kmer("ACG");  // k=3
    // Shift left, drop A, append T: should get "CGT"
    uint64_t shifted = shift_left_append(kmer, 3, encode_base('T'));
    ASSERT_EQ(decode_kmer(shifted, 3), "CGT");
}

TEST(prefix_k4) {
    uint64_t kmer = encode_kmer("ACGT");  // k=4
    uint64_t pfx = prefix(kmer, 4);
    ASSERT_EQ(decode_kmer(pfx, 3), "ACG");
}

TEST(suffix_k4) {
    uint64_t kmer = encode_kmer("ACGT");  // k=4
    uint64_t sfx = suffix(kmer, 4);
    ASSERT_EQ(decode_kmer(sfx, 3), "CGT");
}

// ========== GC and complexity tests ==========

TEST(gc_content_empty) {
    ASSERT_NEAR(gc_content(""), 0.0, 1e-9);
}

TEST(gc_content_allGC) {
    ASSERT_NEAR(gc_content("GC"), 1.0, 1e-9);
}

TEST(gc_content_allAT) {
    ASSERT_NEAR(gc_content("AT"), 0.0, 1e-9);
}

TEST(gc_content_mixed) {
    // "ACGT" = 2 GC out of 4 = 0.5
    ASSERT_NEAR(gc_content("ACGT"), 0.5, 1e-9);
}

TEST(gc_content_lowercase) {
    ASSERT_NEAR(gc_content("gc"), 1.0, 1e-9);
}

TEST(is_valid_sequence) {
    ASSERT_TRUE(is_valid_sequence("ACGT"));
    ASSERT_TRUE(is_valid_sequence("acgtACGT"));
    ASSERT_FALSE(is_valid_sequence("ACGN"));
    ASSERT_FALSE(is_valid_sequence("ACG."));
    ASSERT_TRUE(is_valid_sequence(""));
}

TEST(sequence_complexity_high) {
    // "ACGTACGT" — highly repetitive, complexity should be low.
    double cx = sequence_complexity("ACGTACGT", 3);
    ASSERT_TRUE(cx < 0.5);
}

TEST(sequence_complexity_random) {
    // A longer, diverse sequence should have higher complexity.
    std::string seq = "ACGTACGTACGTACGTACGTACGTACGTACGT";
    double cx = sequence_complexity(seq, 3);
    ASSERT_TRUE(cx > 0.01);  // At least some complexity.
}

TEST(sequence_complexity_random_high) {
    // A truly random-looking sequence should have high complexity.
    std::string seq = "ACGTTCGAACGTTCGAACGTTCGAACGTTCGA";
    double cx = sequence_complexity(seq, 3);
    ASSERT_TRUE(cx > 0.05);
}
