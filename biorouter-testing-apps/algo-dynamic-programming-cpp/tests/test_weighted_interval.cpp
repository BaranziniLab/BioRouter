#include "test_framework.hpp"
#include "dp/weighted_interval.hpp"

TEST_CASE("Weighted Interval — basic") {
    // intervals: [0,3,w=3], [1,4,w=2], [2,5,w=4], [3,6,w=3]
    // sorted by end: [0,3](3), [1,4](2), [2,5](4), [3,6](3)
    // Non-overlapping: {0,3}+{3,6} = 3+3=6; or {2,5}=4; or {1,4}+{4,?)=2
    auto r = dp::weighted_interval({0,1,2,3}, {3,4,5,6}, {3,2,4,3});
    // {0,3}(w=3) + {3,6}(w=3) = 6
    REQUIRE_EQ(r.value, 6LL);
}

TEST_CASE("Weighted Interval — single interval") {
    auto r = dp::weighted_interval({0}, {5}, {10});
    REQUIRE_EQ(r.value, 10LL);
    REQUIRE_EQ(r.solution.size(), 1u);
}

TEST_CASE("Weighted Interval — all overlap") {
    // All start before the first ends → pick the heaviest
    auto r = dp::weighted_interval({0,0,0}, {10,10,10}, {1,5,3});
    REQUIRE_EQ(r.value, 5LL);
}

TEST_CASE("Weighted Interval — none overlap") {
    auto r = dp::weighted_interval({0,10,20}, {5,15,25}, {3,4,5});
    REQUIRE_EQ(r.value, 12LL);
    REQUIRE_EQ(r.solution.size(), 3u);
}

TEST_CASE("Weighted Interval — empty") {
    auto r = dp::weighted_interval({}, {}, {});
    REQUIRE_EQ(r.value, 0LL);
}

TEST_CASE("Weighted Interval — reconstruction non-overlapping") {
    std::vector<int> s = {1,2,3,4,5}, e = {3,5,6,8,9}, w = {5,6,4,7,2};
    auto r = dp::weighted_interval(s, e, w);
    // Verify no overlaps in chosen intervals
    std::vector<std::pair<int,int>> chosen;
    for (int idx : r.solution) chosen.push_back({s[idx], e[idx]});
    std::sort(chosen.begin(), chosen.end());
    for (size_t i = 1; i < chosen.size(); ++i)
        REQUIRE(chosen[i].first >= chosen[i-1].second);
}
