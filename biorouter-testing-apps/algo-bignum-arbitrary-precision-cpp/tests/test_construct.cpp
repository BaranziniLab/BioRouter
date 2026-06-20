// test_construct.cpp — Construction tests
#include "bigint.hpp"
#include "test_framework.hpp"
using namespace bigint;

DEFINE_TEST(test_default_construct) {
    BigInt a;
    TEST_ASSERT(a.is_zero());
    TEST_ASSERT_EQ(a.to_string(), "0");
    TEST_ASSERT_EQ(a.sign(), 0);
}

DEFINE_TEST(test_construct_from_zero) {
    BigInt a(0);
    TEST_ASSERT(a.is_zero());
    TEST_ASSERT_EQ(a.to_string(), "0");
}

DEFINE_TEST(test_construct_positive) {
    BigInt a(42);
    TEST_ASSERT(a.is_positive());
    TEST_ASSERT(!a.is_negative());
    TEST_ASSERT(!a.is_zero());
    TEST_ASSERT_EQ(a.to_string(), "42");
    TEST_ASSERT_EQ(a.sign(), 1);
}

DEFINE_TEST(test_construct_negative) {
    BigInt a(-42);
    TEST_ASSERT(a.is_negative());
    TEST_ASSERT(!a.is_positive());
    TEST_ASSERT_EQ(a.to_string(), "-42");
    TEST_ASSERT_EQ(a.sign(), -1);
}

DEFINE_TEST(test_construct_int64_limits) {
    BigInt a(INT64_MAX);
    TEST_ASSERT_EQ(a.to_string(), "9223372036854775807");

    BigInt b(INT64_MIN);
    TEST_ASSERT_EQ(b.to_string(), "-9223372036854775808");
}

DEFINE_TEST(test_construct_from_string) {
    BigInt a("123456789012345678901234567890");
    TEST_ASSERT_EQ(a.to_string(), "123456789012345678901234567890");

    BigInt b("-99999999999999999999");
    TEST_ASSERT_EQ(b.to_string(), "-99999999999999999999");

    BigInt c("0");
    TEST_ASSERT(c.is_zero());
}

DEFINE_TEST(test_construct_from_hex) {
    BigInt a("0xFF");
    TEST_ASSERT_EQ(a.to_string(), "255");

    BigInt b("0x100000000");  // 2^32
    TEST_ASSERT_EQ(b.to_string(), "4294967296");

    BigInt c("0xdeadbeef");
    TEST_ASSERT_EQ(c.to_hex_string(), "deadbeef");
}

DEFINE_TEST(test_construct_invalid) {
    TEST_ASSERT_THROWS(BigInt(""), std::invalid_argument);
    TEST_ASSERT_THROWS(BigInt("abc"), std::invalid_argument);
    TEST_ASSERT_THROWS(BigInt("12a45"), std::invalid_argument);
}

DEFINE_TEST(test_copy_construct) {
    BigInt a(123456789);
    BigInt b(a);
    TEST_ASSERT_EQ(a, b);
    TEST_ASSERT_EQ(b.to_string(), "123456789");
}

DEFINE_TEST(test_even_odd) {
    BigInt a(4);
    TEST_ASSERT(a.is_even());
    TEST_ASSERT(!a.is_odd());

    BigInt b(7);
    TEST_ASSERT(b.is_odd());
    TEST_ASSERT(!b.is_even());

    BigInt c(0);
    TEST_ASSERT(c.is_even());
}
