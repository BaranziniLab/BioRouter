/**
 * @file test_counter.cpp
 * @brief Tests for hash-map based k-mer counting.
 */

#include "test_framework.hpp"
#include "counter.hpp"
#include "kmer.hpp"
#include <string>

using namespace bkc;

// ========== Construction tests ==========

TEST(counter_construction_valid) {
    KmerCounter c(5);
    ASSERT_EQ(c.k(), 5u);
    ASSERT_EQ(c.unique_count(), 0u);
    ASSERT_EQ(c.total_count(), 0u);
}

TEST(counter_construction_k1) {
    KmerCounter c(1);
    ASSERT_EQ(c.k(), 1u);
}

TEST(counter_construction_k_too_large) {
    ASSERT_THROWS(KmerCounter(MAX_K + 1), std::invalid_argument);
}

TEST(counter_construction_k_zero) {
    ASSERT_THROWS(KmerCounter(0), std::invalid_argument);
}

// ========== Basic counting tests ==========

TEST(counter_single_kmer) {
    KmerCounter c(3);
    c.count("ACG");
    // "ACG" with k=3: only one k-mer.
    ASSERT_EQ(c.total_count(), 1u);
    ASSERT_EQ(c.unique_count(), 1u);

    // Check the count.
    uint64_t kmer = encode_kmer("ACG");
    uint64_t canon = canonical(kmer, 3);
    ASSERT_EQ(c.get_count(canon), 1u);
}

TEST(counter_repeated_kmer) {
    KmerCounter c(3);
    // "ACGACG" contains two k-mers: ACG and CGA (with sliding window).
    // Wait, with k=3: "ACGACG" has 4 k-mers: ACG, CGA, GAC, ACG
    c.count("ACGACG");

    uint64_t acg_canon = canonical(encode_kmer("ACG"), 3);
    // ACG appears twice.
    ASSERT_EQ(c.get_count(acg_canon), 2u);
}

TEST(counter_known_counts_simple) {
    // "AAAA" with k=3:
    // K-mers: AAA, AAA
    // Canonical AAA is AAA (palindrome).
    // So 1 unique k-mer, count = 2.
    KmerCounter c(3);
    c.count("AAAA");
    ASSERT_EQ(c.unique_count(), 1u);
    ASSERT_EQ(c.total_count(), 2u);

    uint64_t aaa_canon = canonical(encode_kmer("AAA"), 3);
    ASSERT_EQ(c.get_count(aaa_canon), 2u);
}

TEST(counter_four_distinct_kmers) {
    // "ACGTACGT" with k=4:
    // K-mers: ACGT, CGTA, GTAC, TACG
    // ACGT: rc = GTAC. ACGT < GTAC. Canon = ACGT.
    // CGTA: rc = TACG. CGTA < TACG. Canon = CGTA.
    // GTAC: rc = ACGT. ACGT < GTAC. Canon = ACGT.
    // TACG: rc = CGTA. CGTA < TACG. Canon = CGTA.
    // So unique: {ACGT, CGTA} = 2 unique.
    // Wait, let me recalculate:
    // ACGT: A(00)C(01)G(10)T(11) = 0b00011011
    //   RC: T(11)G(10)C(01)A(00) = 0b11100100 (GTAC)
    //   ACGT < GTAC? 00011011 < 11100100? Yes. Canon = ACGT.
    // CGTA: C(01)G(10)T(11)A(00) = 0b01101100
    //   RC: T(11)A(00)G(10)C(01) = 0b11001001 (TACG)
    //   01101100 < 11001001? Yes. Canon = CGTA.
    // GTAC: G(10)T(11)A(00)C(01) = 0b10110001
    //   RC: C(01)A(00)T(11)G(10) = 0b01001110 (CAGT? No...)
    //   Wait: GTAC rc = comp(T)comp(A)comp(G)comp(C) reversed = A(00)T(11)C(01)G(10)? No.
    //   Let me be more careful.
    //   GTAC in binary: G=10, T=11, A=00, C=01 -> 10110001
    //   RC: reverse( complement(G) complement(T) complement(A) complement(C) )
    //     = reverse( C=01, A=00, T=11, G=10 )
    //     = reverse( 01001110 ) = 10110001? No wait.
    //   Actually the reverse_complement function reverses bits.
    //   GTAC: 10|11|00|01 = 10110001
    //   RC: extract from LSB: 01(C->G), 00(A->T), 11(T->A), 10(G->C)
    //     building: 01 << 6 | 00 << 4 | 11 << 2 | 10 = 01001110 = 78
    //   So GTAC=10110001=177, RC=01001110=78.
    //   78 < 177, so canon(GTAC) = 78 = 01001110.
    //   01001110 = 01|00|11|10 = C A T G? That's CATG.
    //   Wait, this doesn't match CGTA or ACGT. Let me recheck...
    //   01001110 in 2-bit groups: 01|00|11|10 = C,A,T,G = CATG
    // Hmm, that means canon(GTAC) = CATG, not ACGT.
    // So the 4 canonical k-mers are: ACGT, CGTA, CATG, and... TACG?
    // Let me redo all 4:
    // ACGT=00011011(27), RC=GTAC=10110001(177). 27<177. Canon=ACGT(27).
    // CGTA=01101100(108), RC=TACG=11001001(201). 108<201. Canon=CGTA(108).
    // GTAC=10110001(177), RC=CATG=01001110(78). 78<177. Canon=CATG(78).
    // TACG=11001001(201), RC=CGTA=01101100(108). 108<201. Canon=CGTA(108).
    // So unique canons: {ACGT(27), CGTA(108), CATG(78)} = 3 unique.
    // But total count = 4 (one per position).
    KmerCounter c(4);
    c.count("ACGTACGT");

    // 8 chars, k=4 -> 5 k-mers: ACGT, CGTA, GTAC, TACG, ACGT
    // Unique canons: {ACGT(27), CGTA(108), CATG(78)} = 3 unique.
    ASSERT_EQ(c.total_count(), 5u);
    ASSERT_EQ(c.unique_count(), 3u);
}

TEST(counter_no_sequence_too_short) {
    KmerCounter c(5);
    c.count("ACG");  // length 3 < k=5
    ASSERT_EQ(c.total_count(), 0u);
}

// ========== Spectrum tests ==========

TEST(counter_spectrum_single_entry) {
    KmerCounter c(3);
    c.count("AAAA");
    auto spec = c.spectrum();
    ASSERT_EQ(spec.size(), 1u);
    ASSERT_EQ(spec[0].count, 2u);      // AAA appears 2 times.
    ASSERT_EQ(spec[0].frequency, 1u);  // 1 distinct k-mer has count 2.
}

TEST(counter_spectrum_multiple_entries) {
    KmerCounter c(3);
    // "ACGCG" with k=3:
    // K-mers: ACG, CGC, GCG
    // ACG -> canonical ACG
    // CGC -> rc GCG, canon CGC (wait: CGC rc is GCG. CGC < GCG? 010101 < 101010? Yes. So canon = CGC.)
    // Actually wait: CGC in binary: C=01, G=10, C=01 -> 011001
    // GCG in binary: G=10, C=01, G=10 -> 100110
    // 011001 < 100110, so CGC < GCG. Canon(CGC) = CGC.
    // GCG: same as above, canon = CGC.
    // So ACG once, CGC twice (CGC + GCG map to CGC).
    KmerCounter c2(3);
    c2.count("ACGCG");
    auto spec = c2.spectrum();
    // spec should have entries for count=1 and count=2.
    ASSERT_TRUE(spec.size() >= 2u);

    bool found_c1 = false, found_c2 = false;
    for (auto& e : spec) {
        if (e.count == 1) found_c1 = true;
        if (e.count == 2) found_c2 = true;
    }
    ASSERT_TRUE(found_c1);
    ASSERT_TRUE(found_c2);
}

// ========== Clear tests ==========

TEST(counter_clear) {
    KmerCounter c(3);
    c.count("ACGTACGT");
    ASSERT_TRUE(c.unique_count() > 0);

    c.clear();
    ASSERT_EQ(c.unique_count(), 0u);
    ASSERT_EQ(c.total_count(), 0u);
}

// ========== Manual add tests ==========

TEST(counter_manual_add) {
    KmerCounter c(3);
    uint64_t kmer = canonical(encode_kmer("ACG"), 3);
    c.add(kmer);
    c.add(kmer);
    c.add(kmer);

    ASSERT_EQ(c.get_count(kmer), 3u);
    ASSERT_EQ(c.total_count(), 3u);
    ASSERT_EQ(c.unique_count(), 1u);
}

// ========== Max count ==========

TEST(counter_max_count) {
    KmerCounter c(3);
    uint64_t k1 = canonical(encode_kmer("ACG"), 3);
    uint64_t k2 = canonical(encode_kmer("AAA"), 3);  // Different k-mer

    c.add(k1);
    c.add(k1);
    c.add(k1);
    c.add(k1);  // k1 appears 4 times
    c.add(k2);  // k2 appears 1 time

    ASSERT_EQ(c.max_count(), 4u);
}

// ========== Complex counting scenario ==========

TEST(counter_known_counts_complex) {
    // Sequence: "ATATATAT" with k=3
    // K-mers: ATA, TAT, ATA, TAT, ATA, TAT
    // ATA: A(00)T(11)A(00) = 001100
    //   RC: ATA -> complement TAT -> reverse TAT = 110011
    //   001100 < 110011, so canon(ATA) = ATA.
    // TAT: T(11)A(00)T(11) = 110011
    //   RC: TAT -> complement ATA -> reverse ATA = 001100
    //   001100 < 110011, so canon(TAT) = ATA.
    // So all 6 k-mers map to ATA. 1 unique, count 6.
    KmerCounter c(3);
    c.count("ATATATAT");
    ASSERT_EQ(c.total_count(), 6u);
    ASSERT_EQ(c.unique_count(), 1u);
}
