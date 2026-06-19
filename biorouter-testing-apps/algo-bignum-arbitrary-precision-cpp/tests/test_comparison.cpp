// test_comparison.cpp — Comparison operator tests
#include "bigint.hpp"
#include "test_framework.hpp"
using namespace bigint;

DEFINE_TEST(test_eq_basic) {
    TEST_ASSERT(BigInt(42) == BigInt(42));
    TEST_ASSERT(BigInt(0) == BigInt(0));
    TEST_ASSERT(BigInt(-5) == BigInt(-5));
    TEST_ASSERT(!(BigInt(3) == BigInt(4)));
}

DEFINE_TEST(test_ne_basic) {
    TEST_ASSERT(BigInt(3) != BigInt(4));
    TEST_ASSERT(BigInt(-1) != BigInt(1));
    TEST_ASSERT(!(BigInt(42) != BigInt(42)));
}

DEFINE_TEST(test_lt_basic) {
    TEST_ASSERT(BigInt(1) < BigInt(2));
    TEST_ASSERT(BigInt(-5) < BigInt(0));
    TEST_ASSERT(BigInt(-5) < BigInt(3));
    TEST_ASSERT(!(BigInt(3) < BigInt(3)));
    TEST_ASSERT(!(BigInt(5) < BigInt(3)));
}

DEFINE_TEST(test_gt_basic) {
    TEST_ASSERT(BigInt(5) > BigInt(3));
    TEST_ASSERT(BigInt(0) > BigInt(-1));
    TEST_ASSERT(!(BigInt(3) > BigInt(3)));
}

DEFINE_TEST(test_le_ge) {
    TEST_ASSERT(BigInt(3) <= BigInt(3));
    TEST_ASSERT(BigInt(3) <= BigInt(4));
    TEST_ASSERT(BigInt(4) >= BigInt(3));
    TEST_ASSERT(BigInt(4) >= BigInt(4));
}

DEFINE_TEST(test_cmp_large) {
    BigInt a("999999999999999999999999999999");
    BigInt b("1000000000000000000000000000000");
    TEST_ASSERT(a < b);
    TEST_ASSERT(b > a);
    TEST_ASSERT(a != b);
}

DEFINE_TEST(test_cmp_cross_sign) {
    BigInt pos("10000000000000000000000");
    BigInt neg("-10000000000000000000000");
    TEST_ASSERT(neg < pos);
    TEST_ASSERT(pos > neg);
    TEST_ASSERT(neg < BigInt(0));
    TEST_ASSERT(pos > BigInt(0));
}
