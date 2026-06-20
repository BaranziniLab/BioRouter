#include "test_framework.hpp"
#include "dp/knapsack_01.hpp"

TEST_CASE("Knapsack 0/1 — basic") {
    // items: (w=2,v=3), (w=3,v=4), (w=4,v=5), (w=5,v=6); cap=5
    auto r = dp::knapsack_01({2,3,4,5}, {3,4,5,6}, 5);
    REQUIRE_EQ(r.value, 7LL); // items 0+1
    REQUIRE_EQ(r.solution.size(), 2u);
}

TEST_CASE("Knapsack 0/1 — zero capacity") {
    auto r = dp::knapsack_01({1,2}, {3,4}, 0);
    REQUIRE_EQ(r.value, 0LL);
    REQUIRE(r.solution.empty());
}

TEST_CASE("Knapsack 0/1 — empty items") {
    auto r = dp::knapsack_01({}, {}, 10);
    REQUIRE_EQ(r.value, 0LL);
}

TEST_CASE("Knapsack 0/1 — all fit") {
    auto r = dp::knapsack_01({1,1,1}, {10,20,30}, 10);
    REQUIRE_EQ(r.value, 60LL);
    REQUIRE_EQ(r.solution.size(), 3u);
}

TEST_CASE("Knapsack 0/1 — single item fits") {
    auto r = dp::knapsack_01({5}, {10}, 5);
    REQUIRE_EQ(r.value, 10LL);
    REQUIRE_EQ(r.solution.size(), 1u);
}

TEST_CASE("Knapsack 0/1 — single item too heavy") {
    auto r = dp::knapsack_01({6}, {10}, 5);
    REQUIRE_EQ(r.value, 0LL);
    REQUIRE(r.solution.empty());
}

TEST_CASE("Knapsack 0/1 — reconstruction correctness") {
    auto r = dp::knapsack_01({2,3,4,5}, {3,4,5,6}, 7);
    // Best: items 1 (w=3,v=4) + item 2 (w=4,v=5) = w=7, v=9
    REQUIRE_EQ(r.value, 9LL);
    long long wsum = 0, vsum = 0;
    std::vector<int> w = {2,3,4,5}, v = {3,4,5,6};
    for (int idx : r.solution) { wsum += w[idx]; vsum += v[idx]; }
    REQUIRE(wsum <= 7);
    REQUIRE_EQ(vsum, 9LL);
}

TEST_CASE("Knapsack 0/1 — large values") {
    auto r = dp::knapsack_01({10,20,30}, {60,100,120}, 50);
    REQUIRE_EQ(r.value, 220LL); // items 1+2
}
