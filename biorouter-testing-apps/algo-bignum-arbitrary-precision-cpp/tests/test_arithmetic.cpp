// test_arithmetic.cpp — Addition, subtraction, multiplication tests
#include "bigint.hpp"
#include "test_framework.hpp"
using namespace bigint;

DEFINE_TEST(test_add_basic) {
    TEST_ASSERT_EQ(BigInt(3) + BigInt(5), BigInt(8));
    TEST_ASSERT_EQ(BigInt(0) + BigInt(0), BigInt(0));
    TEST_ASSERT_EQ(BigInt(100) + BigInt(0), BigInt(100));
}

DEFINE_TEST(test_add_carry) {
    // 2^32 - 1 + 1 = 2^32
    BigInt a("4294967295");
    BigInt b(1);
    TEST_ASSERT_EQ((a + b).to_string(), "4294967296");

    // Multi-limb carry propagation
    BigInt c("18446744073709551615"); // 2^64 - 1
    TEST_ASSERT_EQ((c + BigInt(1)).to_string(), "18446744073709551616");
}

DEFINE_TEST(test_add_large) {
    BigInt a("999999999999999999999999999999");
    BigInt b("1");
    TEST_ASSERT_EQ((a + b).to_string(), "1000000000000000000000000000000");
}

DEFINE_TEST(test_sub_basic) {
    TEST_ASSERT_EQ(BigInt(8) - BigInt(3), BigInt(5));
    TEST_ASSERT_EQ(BigInt(100) - BigInt(100), BigInt(0));
}

DEFINE_TEST(test_sub_borrow) {
    BigInt a("4294967296"); // 2^32
    BigInt b(1);
    TEST_ASSERT_EQ((a - b).to_string(), "4294967295");

    BigInt c("100000000000000000000");
    BigInt d("1");
    TEST_ASSERT_EQ((c - d).to_string(), "99999999999999999999");
}

DEFINE_TEST(test_sub_result_negative) {
    TEST_ASSERT_EQ(BigInt(3) - BigInt(5), BigInt(-2));
    TEST_ASSERT_EQ(BigInt(0) - BigInt(1), BigInt(-1));
}

DEFINE_TEST(test_mul_basic) {
    TEST_ASSERT_EQ(BigInt(6) * BigInt(7), BigInt(42));
    TEST_ASSERT_EQ(BigInt(0) * BigInt(100), BigInt(0));
    TEST_ASSERT_EQ(BigInt(1) * BigInt(100), BigInt(100));
    TEST_ASSERT_EQ(BigInt(-3) * BigInt(5), BigInt(-15));
    TEST_ASSERT_EQ(BigInt(-3) * BigInt(-5), BigInt(15));
}

DEFINE_TEST(test_mul_large) {
    // 2^32 * 2^32 = 2^64
    BigInt a("4294967296");
    BigInt b("4294967296");
    TEST_ASSERT_EQ((a * b).to_string(), "18446744073709551616");

    // 10^18 * 10^18 = 10^36
    BigInt c("1000000000000000000");
    BigInt d("1000000000000000000");
    TEST_ASSERT_EQ((c * d).to_string(), "1000000000000000000000000000000000000");
}

DEFINE_TEST(test_mul_power_of_two) {
    // 2^100
    BigInt two(2);
    BigInt result(1);
    for (int i = 0; i < 100; ++i) result = result * two;
    TEST_ASSERT_EQ(result.to_string(), "1267650600228229401496703205376");
}

DEFINE_TEST(test_unary_neg) {
    BigInt a(42);
    BigInt b = -a;
    TEST_ASSERT_EQ(b.to_string(), "-42");
    TEST_ASSERT_EQ(-b, a);

    BigInt zero;
    TEST_ASSERT_EQ((-zero).to_string(), "0");
}

DEFINE_TEST(test_increment_decrement) {
    BigInt a(99);
    TEST_ASSERT_EQ((++a).to_string(), "100");
    TEST_ASSERT_EQ(a.to_string(), "100");

    BigInt b(100);
    BigInt c = b++;
    TEST_ASSERT_EQ(c.to_string(), "100");
    TEST_ASSERT_EQ(b.to_string(), "101");

    BigInt d(1);
    TEST_ASSERT_EQ((--d).to_string(), "0");
    TEST_ASSERT(d.is_zero());
}

DEFINE_TEST(test_mul_commutative) {
    BigInt a("123456789012345678901234567890");
    BigInt b("987654321098765432109876543210");
    TEST_ASSERT_EQ(a * b, b * a);
}

DEFINE_TEST(test_mul_associative_small) {
    BigInt a(2), b(3), c(4);
    TEST_ASSERT_EQ((a * b) * c, a * (b * c));
}

DEFINE_TEST(test_mul_distributive) {
    BigInt a(7), b(11), c(13);
    TEST_ASSERT_EQ(a * (b + c), a * b + a * c);
}
