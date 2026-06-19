#include "test_framework.hpp"
#include "dp/lis.hpp"

TEST_CASE("LIS — basic") {
    auto r = dp::lis({10, 9, 2, 5, 3, 7, 101, 18});
    REQUIRE_EQ(r.value, 4LL); // {2,3,7,101} or {2,5,7,101} etc.
}

TEST_CASE("LIS — strictly increasing") {
    auto r = dp::lis({1,2,3,4,5});
    REQUIRE_EQ(r.value, 5LL);
    REQUIRE_EQ(r.solution.size(), 5u);
}

TEST_CASE("LIS — strictly decreasing") {
    auto r = dp::lis({5,4,3,2,1});
    REQUIRE_EQ(r.value, 1LL);
    REQUIRE_EQ(r.solution.size(), 1u);
}

TEST_CASE("LIS — empty sequence") {
    auto r = dp::lis({});
    REQUIRE_EQ(r.value, 0LL);
    REQUIRE(r.solution.empty());
}

TEST_CASE("LIS — single element") {
    auto r = dp::lis({42});
    REQUIRE_EQ(r.value, 1LL);
    REQUIRE_EQ(r.solution.size(), 1u);
    REQUIRE_EQ(r.solution[0], 0);
}

TEST_CASE("LIS — duplicates") {
    auto r = dp::lis({3,3,3,3});
    REQUIRE_EQ(r.value, 1LL);
}

TEST_CASE("LIS — reconstruction is valid increasing subsequence") {
    std::vector<int> seq = {10, 9, 2, 5, 3, 7, 101, 18};
    auto r = dp::lis(seq);
    // Indices must be strictly increasing
    for (size_t i = 1; i < r.solution.size(); ++i)
        REQUIRE(r.solution[i] > r.solution[i-1]);
    // Values at those indices must be strictly increasing
    for (size_t i = 1; i < r.solution.size(); ++i)
        REQUIRE(seq[r.solution[i]] > seq[r.solution[i-1]]);
    REQUIRE_EQ(r.solution.size(), (size_t)r.value);
}

TEST_CASE("LIS — classic example") {
    auto r = dp::lis({0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15});
    REQUIRE_EQ(r.value, 6LL);
}
