#include "test_framework.hpp"
#include "dp/coin_change.hpp"
#include <algorithm>
#include <numeric>

TEST_CASE("Coin Change Min — basic") {
    auto r = dp::coin_change_min({1,5,10,25}, 30);
    REQUIRE_EQ(r.value, 2LL); // 25+5
}

TEST_CASE("Coin Change Min — impossible") {
    auto r = dp::coin_change_min({2}, 3);
    REQUIRE_EQ(r.value, -1LL);
}

TEST_CASE("Coin Change Min — zero amount") {
    auto r = dp::coin_change_min({1,5}, 0);
    REQUIRE_EQ(r.value, 0LL);
}

TEST_CASE("Coin Change Min — single coin") {
    auto r = dp::coin_change_min({3}, 9);
    REQUIRE_EQ(r.value, 3LL);
    for (int c : r.solution) REQUIRE_EQ(c, 3);
}

TEST_CASE("Coin Change Min — reconstruction sums correctly") {
    auto r = dp::coin_change_min({1,5,10,25}, 41);
    int sum = 0;
    for (int c : r.solution) sum += c;
    REQUIRE_EQ(sum, 41);
    REQUIRE_EQ(r.value, (long long)r.solution.size());
}

TEST_CASE("Coin Change Count — basic") {
    // ways to make 5 with {1,2,5}: {5},{2,2,1},{2,1,1,1},{1,1,1,1,1} = 4
    auto r = dp::coin_change_count({1,2,5}, 5);
    REQUIRE_EQ(r.value, 4LL);
}

TEST_CASE("Coin Change Count — zero amount") {
    auto r = dp::coin_change_count({1,2}, 0);
    REQUIRE_EQ(r.value, 1LL); // one way: use nothing
}

TEST_CASE("Coin Change Count — impossible") {
    auto r = dp::coin_change_count({2}, 3);
    REQUIRE_EQ(r.value, 0LL);
}

TEST_CASE("Coin Change Count — single coin") {
    auto r = dp::coin_change_count({1}, 5);
    REQUIRE_EQ(r.value, 1LL);
}
