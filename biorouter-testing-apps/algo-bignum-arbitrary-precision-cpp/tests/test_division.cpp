// test_division.cpp — Division and modulo tests
#include "bigint.hpp"
#include "test_framework.hpp"
using namespace bigint;

DEFINE_TEST(test_div_basic) {
    TEST_ASSERT_EQ(BigInt(10) / BigInt(3), BigInt(3));
    TEST_ASSERT_EQ(BigInt(10) / BigInt(2), BigInt(5));
    TEST_ASSERT_EQ(BigInt(0) / BigInt(5), BigInt(0));
    TEST_ASSERT_EQ(BigInt(7) / BigInt(1), BigInt(7));
}

DEFINE_TEST(test_div_large) {
    BigInt a("1000000000000000000000000000000");
    BigInt b("1000000000000000000");
    TEST_ASSERT_EQ((a / b).to_string(), "1000000000000");
}

DEFINE_TEST(test_div_exact) {
    // 2^64 / 2^32 = 2^32
    BigInt a("18446744073709551616");
    BigInt b("4294967296");
    TEST_ASSERT_EQ((a / b).to_string(), "4294967296");
}

DEFINE_TEST(test_div_by_zero) {
    TEST_ASSERT_THROWS(BigInt(5) / BigInt(0), std::domain_error);
}

DEFINE_TEST(test_mod_basic) {
    TEST_ASSERT_EQ(BigInt(10) % BigInt(3), BigInt(1));
    TEST_ASSERT_EQ(BigInt(10) % BigInt(2), BigInt(0));
    TEST_ASSERT_EQ(BigInt(7) % BigInt(7), BigInt(0));
}

DEFINE_TEST(test_mod_large) {
    // 10^36 % (10^18 + 7)
    BigInt a("1000000000000000000000000000000000000");
    BigInt b("1000000000000000007");
    BigInt q = a / b;
    BigInt r = a % b;
    // a = q * b + r
    TEST_ASSERT_EQ(q * b + r, a);
}

DEFINE_TEST(test_mod_by_zero) {
    TEST_ASSERT_THROWS(BigInt(5) % BigInt(0), std::domain_error);
}

DEFINE_TEST(test_divmod_consistency) {
    // For any a, b: a = (a/b)*b + (a%b)
    auto check = [&test_name](int64_t av, int64_t bv) {
        if (bv == 0) return;
        BigInt a(av), b(bv);
        BigInt q = a / b;
        BigInt r = a % b;
        TEST_ASSERT_EQ(q * b + r, a);
    };
    check(100, 7);
    check(100, -7);
    check(-100, 7);
    check(-100, -7);
    check(0, 5);
    check(123456789, 9876);
}

DEFINE_TEST(test_div_signs) {
    TEST_ASSERT_EQ(BigInt(10) / BigInt(3), BigInt(3));
    TEST_ASSERT_EQ(BigInt(-10) / BigInt(3), BigInt(-3));
    TEST_ASSERT_EQ(BigInt(10) / BigInt(-3), BigInt(-3));
    TEST_ASSERT_EQ(BigInt(-10) / BigInt(-3), BigInt(3));
}

DEFINE_TEST(test_div_multi_limb) {
    // Divide a multi-limb number by another
    BigInt a("79228162514264337593543950335"); // near 2^96
    BigInt b("4294967295"); // 2^32 - 1
    BigInt q = a / b;
    BigInt r = a % b;
    TEST_ASSERT_EQ(q * b + r, a);
}

DEFINE_TEST(test_div_single_limb_edge) {
    // Division where divisor fits in one limb
    BigInt a("99999999999999999999999999999999"); // 10^32
    BigInt b("3");
    BigInt q = a / b;
    BigInt r = a % b;
    TEST_ASSERT_EQ(q * b + r, a);
    TEST_ASSERT_EQ(r.to_string(), "1");
}
