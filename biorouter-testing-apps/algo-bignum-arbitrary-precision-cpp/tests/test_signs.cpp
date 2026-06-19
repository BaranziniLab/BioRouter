// test_signs.cpp — Sign edge cases in all operations
#include "bigint.hpp"
#include "test_framework.hpp"
using namespace bigint;

DEFINE_TEST(test_sign_add_same_sign) {
    TEST_ASSERT_EQ(BigInt(3) + BigInt(5), BigInt(8));
    TEST_ASSERT_EQ(BigInt(-3) + BigInt(-5), BigInt(-8));
}

DEFINE_TEST(test_sign_add_diff_sign) {
    TEST_ASSERT_EQ(BigInt(5) + BigInt(-3), BigInt(2));
    TEST_ASSERT_EQ(BigInt(-5) + BigInt(3), BigInt(-2));
    TEST_ASSERT_EQ(BigInt(3) + BigInt(-5), BigInt(-2));
    TEST_ASSERT_EQ(BigInt(-3) + BigInt(5), BigInt(2));
    TEST_ASSERT_EQ(BigInt(3) + BigInt(-3), BigInt(0));
}

DEFINE_TEST(test_sign_sub) {
    TEST_ASSERT_EQ(BigInt(5) - BigInt(3), BigInt(2));
    TEST_ASSERT_EQ(BigInt(3) - BigInt(5), BigInt(-2));
    TEST_ASSERT_EQ(BigInt(-3) - BigInt(5), BigInt(-8));
    TEST_ASSERT_EQ(BigInt(-3) - BigInt(-5), BigInt(2));
    TEST_ASSERT_EQ(BigInt(5) - BigInt(-3), BigInt(8));
}

DEFINE_TEST(test_sign_mul) {
    TEST_ASSERT_EQ(BigInt(3) * BigInt(5), BigInt(15));
    TEST_ASSERT_EQ(BigInt(-3) * BigInt(5), BigInt(-15));
    TEST_ASSERT_EQ(BigInt(3) * BigInt(-5), BigInt(-15));
    TEST_ASSERT_EQ(BigInt(-3) * BigInt(-5), BigInt(15));
    TEST_ASSERT_EQ(BigInt(0) * BigInt(5), BigInt(0));
    TEST_ASSERT_EQ(BigInt(5) * BigInt(0), BigInt(0));
}

DEFINE_TEST(test_sign_div) {
    TEST_ASSERT_EQ(BigInt(7) / BigInt(3), BigInt(2));
    TEST_ASSERT_EQ(BigInt(-7) / BigInt(3), BigInt(-2));
    TEST_ASSERT_EQ(BigInt(7) / BigInt(-3), BigInt(-2));
    TEST_ASSERT_EQ(BigInt(-7) / BigInt(-3), BigInt(2));
}

DEFINE_TEST(test_sign_mod) {
    TEST_ASSERT_EQ(BigInt(7) % BigInt(3), BigInt(1));
    TEST_ASSERT_EQ(BigInt(-7) % BigInt(3), BigInt(-1));
    TEST_ASSERT_EQ(BigInt(7) % BigInt(-3), BigInt(1));
    TEST_ASSERT_EQ(BigInt(-7) % BigInt(-3), BigInt(-1));
}

DEFINE_TEST(test_sign_comparison) {
    TEST_ASSERT(BigInt(-1) < BigInt(0));
    TEST_ASSERT(BigInt(0) < BigInt(1));
    TEST_ASSERT(BigInt(-5) < BigInt(-3));
    TEST_ASSERT(BigInt(-3) > BigInt(-5));
    TEST_ASSERT(BigInt(0) == BigInt(-0));
}

DEFINE_TEST(test_sign_unary_minus) {
    BigInt a(42);
    TEST_ASSERT_EQ(-a, BigInt(-42));
    TEST_ASSERT_EQ(-(-a), a);
    TEST_ASSERT_EQ(-BigInt(0), BigInt(0));
}

DEFINE_TEST(test_sign_increment_zero) {
    BigInt a(0);
    ++a;
    TEST_ASSERT_EQ(a, BigInt(1));
    --a;
    TEST_ASSERT(a.is_zero());
    --a;
    TEST_ASSERT_EQ(a, BigInt(-1));
}
