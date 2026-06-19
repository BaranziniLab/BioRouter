#include "test_framework.hpp"
#include "dp/subset_sum.hpp"
#include <numeric>

TEST_CASE("Subset Sum — basic feasible") {
    auto r = dp::subset_sum({3, 34, 4, 12, 5, 2}, 9);
    REQUIRE_EQ(r.value, 1LL);
    int sum = 0;
    for (int x : r.solution) sum += x;
    REQUIRE_EQ(sum, 9);
}

TEST_CASE("Subset Sum — infeasible") {
    auto r = dp::subset_sum({1,2,3}, 7);
    REQUIRE_EQ(r.value, 0LL);
    REQUIRE(r.solution.empty());
}

TEST_CASE("Subset Sum — zero target") {
    auto r = dp::subset_sum({1,2,3}, 0);
    REQUIRE_EQ(r.value, 1LL);
}

TEST_CASE("Subset Sum — empty set zero target") {
    auto r = dp::subset_sum({}, 0);
    REQUIRE_EQ(r.value, 1LL);
}

TEST_CASE("Subset Sum — empty set nonzero target") {
    auto r = dp::subset_sum({}, 5);
    REQUIRE_EQ(r.value, 0LL);
}

TEST_CASE("Subset Sum — all elements needed") {
    auto r = dp::subset_sum({1,2,3}, 6);
    REQUIRE_EQ(r.value, 1LL);
    int sum = 0;
    for (int x : r.solution) sum += x;
    REQUIRE_EQ(sum, 6);
}

TEST_CASE("Equal Partition — feasible") {
    auto r = dp::equal_partition({1,5,11,5});
    REQUIRE_EQ(r.value, 1LL);
    int sum = 0;
    for (int x : r.solution) sum += x;
    REQUIRE_EQ(sum, 11); // total=22, half=11
}

TEST_CASE("Equal Partition — odd sum") {
    auto r = dp::equal_partition({1,2,4});
    REQUIRE_EQ(r.value, 0LL);
}

TEST_CASE("Equal Partition — two elements equal") {
    auto r = dp::equal_partition({5,5});
    REQUIRE_EQ(r.value, 1LL);
}

TEST_CASE("Equal Partition — single element") {
    auto r = dp::equal_partition({1});
    REQUIRE_EQ(r.value, 0LL);
}
