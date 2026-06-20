// test_math.cpp — pow, modpow, gcd tests
#include "bigint.hpp"
#include "test_framework.hpp"
using namespace bigint;

DEFINE_TEST(test_pow_basic) {
    TEST_ASSERT_EQ(BigInt::pow(BigInt(2), 0), BigInt(1));
    TEST_ASSERT_EQ(BigInt::pow(BigInt(2), 1), BigInt(2));
    TEST_ASSERT_EQ(BigInt::pow(BigInt(2), 10), BigInt(1024));
    TEST_ASSERT_EQ(BigInt::pow(BigInt(3), 3), BigInt(27));
    TEST_ASSERT_EQ(BigInt::pow(BigInt(0), 5), BigInt(0));
}

DEFINE_TEST(test_pow_large) {
    // 2^256
    BigInt result = BigInt::pow(BigInt(2), 256);
    std::string expected = "115792089237316195423570985008687907853269984665640564039457584007913129639936";
    TEST_ASSERT_EQ(result.to_string(), expected);
}

DEFINE_TEST(test_pow_ten) {
    // 10^100
    BigInt result = BigInt::pow(BigInt(10), 100);
    std::string s = result.to_string();
    TEST_ASSERT_EQ(s.size(), 101u); // "1" + 100 zeros
    TEST_ASSERT_EQ(s[0], '1');
    for (size_t i = 1; i < s.size(); ++i) TEST_ASSERT_EQ(s[i], '0');
}

DEFINE_TEST(test_modpow_basic) {
    // 2^10 mod 1000 = 1024 mod 1000 = 24
    TEST_ASSERT_EQ(BigInt::modpow(BigInt(2), BigInt(10), BigInt(1000)), BigInt(24));

    // 3^13 mod 1000 = 1594323 mod 1000 = 323
    TEST_ASSERT_EQ(BigInt::modpow(BigInt(3), BigInt(13), BigInt(1000)), BigInt(323));
}

DEFINE_TEST(test_modpow_large) {
    // RSA-like: compute a^b mod m for large numbers
    BigInt base("123456789012345678901234567890");
    BigInt exp("987654321098765432109876543210");
    BigInt mod("1000000000000000000000000000000000000");

    BigInt result = BigInt::modpow(base, exp, mod);
    // Result should be in [0, mod)
    TEST_ASSERT_GE(result, BigInt(0));
    TEST_ASSERT_LT(result, mod);
}

DEFINE_TEST(test_modpow_by_one) {
    TEST_ASSERT_EQ(BigInt::modpow(BigInt(999), BigInt(999), BigInt(1)), BigInt(0));
}

DEFINE_TEST(test_modpow_zero_exp) {
    TEST_ASSERT_EQ(BigInt::modpow(BigInt(42), BigInt(0), BigInt(7)), BigInt(1));
}

DEFINE_TEST(test_modpow_by_zero) {
    TEST_ASSERT_THROWS(BigInt::modpow(BigInt(2), BigInt(3), BigInt(0)), std::domain_error);
}

DEFINE_TEST(test_gcd_basic) {
    TEST_ASSERT_EQ(BigInt::gcd(BigInt(12), BigInt(8)), BigInt(4));
    TEST_ASSERT_EQ(BigInt::gcd(BigInt(7), BigInt(5)), BigInt(1));
    TEST_ASSERT_EQ(BigInt::gcd(BigInt(0), BigInt(5)), BigInt(5));
    TEST_ASSERT_EQ(BigInt::gcd(BigInt(5), BigInt(0)), BigInt(5));
    TEST_ASSERT_EQ(BigInt::gcd(BigInt(0), BigInt(0)), BigInt(0));
}

DEFINE_TEST(test_gcd_negative) {
    TEST_ASSERT_EQ(BigInt::gcd(BigInt(-12), BigInt(8)), BigInt(4));
    TEST_ASSERT_EQ(BigInt::gcd(BigInt(12), BigInt(-8)), BigInt(4));
    TEST_ASSERT_EQ(BigInt::gcd(BigInt(-12), BigInt(-8)), BigInt(4));
}

DEFINE_TEST(test_gcd_large) {
    // gcd(2^100, 2^50 * 3) = 2^50
    BigInt a = BigInt::pow(BigInt(2), 100);
    BigInt b = BigInt::pow(BigInt(2), 50) * BigInt(3);
    BigInt expected = BigInt::pow(BigInt(2), 50);
    TEST_ASSERT_EQ(BigInt::gcd(a, b), expected);
}
