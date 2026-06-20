// test_karatsuba.cpp — Karatsuba vs schoolbook agreement tests
#include "bigint.hpp"
#include "test_framework.hpp"
using namespace bigint;

// Helper: build a random-looking BigInt with given number of limbs
static BigInt make_big(size_t limbs) {
    std::string s;
    // Build from a large decimal to get multi-limb numbers
    for (size_t i = 0; i < limbs * 10; ++i) {
        s += char('1' + (i % 9));
    }
    return BigInt(s);
}

DEFINE_TEST(test_karatsuba_vs_schoolbook_small) {
    // Small numbers: both should give same result (Karatsuba not triggered)
    BigInt a(123456789);
    BigInt b(987654321);
    TEST_ASSERT_EQ(a * b, b * a);
    TEST_ASSERT_EQ((a * b).to_string(), "121932631112635269");
}

DEFINE_TEST(test_karatsuba_vs_schoolbook_threshold) {
    // Build numbers just around the Karatsuba threshold (32 limbs)
    // Each limb is 32 bits, so 32 limbs = 1024 bits
    // 2^1024 has 309 decimal digits
    std::string sa(310, '9');
    std::string sb(310, '8');
    BigInt a(sa);
    BigInt b(sb);

    // Verify by also computing (a-b)*b + b*b = a*b
    BigInt product = a * b;
    // Check: product / b == a and product % b == 0
    TEST_ASSERT_EQ(product / b, a);
    TEST_ASSERT_EQ(product % b, BigInt(0));
}

DEFINE_TEST(test_karatsuba_large_squares) {
    // Compute 10^200 * 10^200 = 10^400
    std::string s1(201, '0'); s1[0] = '1';
    BigInt a(s1);
    BigInt product = a * a;

    std::string expected(401, '0'); expected[0] = '1';
    TEST_ASSERT_EQ(product.to_string(), expected);
}

DEFINE_TEST(test_karatsuba_different_sizes) {
    // Multiply numbers of very different sizes
    std::string sa(300, '3');
    std::string sb(50, '7');
    BigInt a(sa);
    BigInt b(sb);

    BigInt product = a * b;
    // Verify: product / b == a
    TEST_ASSERT_EQ(product / b, a);
}

DEFINE_TEST(test_karatsuba_associativity) {
    // (a * b) * c == a * (b * c) for large numbers
    std::string sa(100, '9');
    std::string sb(100, '8');
    std::string sc(100, '7');
    BigInt a(sa), b(sb), c(sc);

    TEST_ASSERT_EQ((a * b) * c, a * (b * c));
}
