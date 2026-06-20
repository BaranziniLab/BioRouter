#include "test_framework.hpp"
#include "dp/knapsack_unbounded.hpp"

TEST_CASE("Unbounded Knapsack — basic") {
    // items: (w=1,v=1), (w=3,v=4); cap=5
    // Best: 1×item1(w=3,v=4) + 2×item0(w=2,v=2) = w=5, v=6
    auto r = dp::knapsack_unbounded({1,3}, {1,4}, 5);
    REQUIRE_EQ(r.value, 6LL);
}

TEST_CASE("Unbounded Knapsack — single item repeated") {
    auto r = dp::knapsack_unbounded({2}, {3}, 10);
    REQUIRE_EQ(r.value, 15LL); // 5 copies
    REQUIRE_EQ(r.solution.size(), 5u);
}

TEST_CASE("Unbounded Knapsack — zero capacity") {
    auto r = dp::knapsack_unbounded({1,2}, {3,4}, 0);
    REQUIRE_EQ(r.value, 0LL);
    REQUIRE(r.solution.empty());
}

TEST_CASE("Unbounded Knapsack — empty items") {
    auto r = dp::knapsack_unbounded({}, {}, 10);
    REQUIRE_EQ(r.value, 0LL);
}

TEST_CASE("Unbounded Knapsack — reconstruction is valid") {
    auto r = dp::knapsack_unbounded({2,5}, {3,7}, 13);
    // Verify weight sum doesn't exceed capacity
    std::vector<int> w = {2,5};
    int wsum = 0;
    for (int idx : r.solution) wsum += w[idx];
    REQUIRE(wsum <= 13);
    // Verify value sum matches
    std::vector<int> v = {3,7};
    long long vsum = 0;
    for (int idx : r.solution) vsum += v[idx];
    REQUIRE_EQ(vsum, r.value);
}
