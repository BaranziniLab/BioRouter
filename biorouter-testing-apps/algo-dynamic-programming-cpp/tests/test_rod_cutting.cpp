#include "test_framework.hpp"
#include "dp/rod_cutting.hpp"

TEST_CASE("Rod Cutting — basic") {
    // prices: length 1→1, 2→5, 3→8, 4→9, 5→10, 6→17, 7→17, 8→20
    std::vector<int> prices = {1,5,8,9,10,17,17,20};
    auto r = dp::rod_cutting(prices);
    REQUIRE_EQ(r.value, 22LL); // 2+6 = 5+17 = 22
}

TEST_CASE("Rod Cutting — single piece") {
    std::vector<int> prices = {3};
    auto r = dp::rod_cutting(prices);
    REQUIRE_EQ(r.value, 3LL);
    REQUIRE_EQ(r.solution.size(), 1u);
    REQUIRE_EQ(r.solution[0], 1);
}

TEST_CASE("Rod Cutting — empty") {
    auto r = dp::rod_cutting({});
    REQUIRE_EQ(r.value, 0LL);
}

TEST_CASE("Rod Cutting — all length-1 optimal") {
    std::vector<int> prices = {3,5,6,7}; // 4×3=12 vs 2×5=10 vs 6+3=9 etc
    auto r = dp::rod_cutting(prices);
    // 4×3=12, 2×5=10, 2×3+6=12, so 12
    REQUIRE_EQ(r.value, 12LL);
    // Verify pieces sum to rod length
    int sum = 0;
    for (int p : r.solution) sum += p;
    REQUIRE_EQ(sum, 4);
}

TEST_CASE("Rod Cutting — reconstruction sums correctly") {
    std::vector<int> prices = {1,5,8,9,10,17,17,20};
    auto r = dp::rod_cutting(prices);
    int sum = 0;
    for (int p : r.solution) sum += p;
    REQUIRE_EQ(sum, 8); // rod length
}

TEST_CASE("Rod Cutting — all same price") {
    std::vector<int> prices = {2,2,2};
    auto r = dp::rod_cutting(prices);
    REQUIRE_EQ(r.value, 6LL); // 3×2
    REQUIRE_EQ(r.solution.size(), 3u);
}
