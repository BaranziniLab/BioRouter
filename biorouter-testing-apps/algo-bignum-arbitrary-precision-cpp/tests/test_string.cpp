// test_string.cpp — String conversion round-trip tests
#include "bigint.hpp"
#include "test_framework.hpp"
using namespace bigint;

DEFINE_TEST(test_to_string_zero) {
    TEST_ASSERT_EQ(BigInt(0).to_string(), "0");
    TEST_ASSERT_EQ(BigInt(0).to_hex_string(), "0");
}

DEFINE_TEST(test_to_string_basic) {
    TEST_ASSERT_EQ(BigInt(42).to_string(), "42");
    TEST_ASSERT_EQ(BigInt(-42).to_string(), "-42");
}

DEFINE_TEST(test_to_string_large) {
    std::string s = "1234567890123456789012345678901234567890";
    BigInt a(s);
    TEST_ASSERT_EQ(a.to_string(), s);
}

DEFINE_TEST(test_to_string_negative_large) {
    std::string s = "-999999999999999999999999999999999999";
    BigInt a(s);
    TEST_ASSERT_EQ(a.to_string(), s);
}

DEFINE_TEST(test_hex_roundtrip) {
    BigInt a("0xdeadbeefcafebabe");
    TEST_ASSERT_EQ(a.to_hex_string(), "deadbeefcafebabe");
}

DEFINE_TEST(test_hex_basic) {
    TEST_ASSERT_EQ(BigInt(255).to_hex_string(), "ff");
    TEST_ASSERT_EQ(BigInt(16).to_hex_string(), "10");
    TEST_ASSERT_EQ(BigInt(10).to_hex_string(), "a");
}

DEFINE_TEST(test_decimal_roundtrip) {
    // Round-trip: parse -> to_string -> parse -> to_string
    std::string orig = "3141592653589793238462643383279502884197";
    BigInt a(orig);
    std::string s1 = a.to_string();
    BigInt b(s1);
    std::string s2 = b.to_string();
    TEST_ASSERT_EQ(s1, s2);
    TEST_ASSERT_EQ(s1, orig);
}

DEFINE_TEST(test_hex_power_of_two) {
    // 2^32 = 0x100000000
    BigInt a("4294967296");
    TEST_ASSERT_EQ(a.to_hex_string(), "100000000");
}

DEFINE_TEST(test_string_edge_single_digit) {
    for (int i = 0; i <= 9; ++i) {
        BigInt a(i);
        TEST_ASSERT_EQ(a.to_string(), std::to_string(i));
    }
}
