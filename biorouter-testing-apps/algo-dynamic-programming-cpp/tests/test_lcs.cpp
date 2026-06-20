#include "test_framework.hpp"
#include "dp/lcs.hpp"

TEST_CASE("LCS — basic strings") {
    auto r = dp::lcs(std::string("ABCBDAB"), std::string("BDCABA"));
    REQUIRE_EQ(r.value, 4LL); // "BCBA" or "BDAB"
}

TEST_CASE("LCS — identical strings") {
    auto r = dp::lcs(std::string("ABCD"), std::string("ABCD"));
    REQUIRE_EQ(r.value, 4LL);
    REQUIRE_EQ(r.solution.size(), 4u);
}

TEST_CASE("LCS — no common subsequence") {
    auto r = dp::lcs(std::string("ABC"), std::string("XYZ"));
    REQUIRE_EQ(r.value, 0LL);
    REQUIRE(r.solution.empty());
}

TEST_CASE("LCS — empty string") {
    auto r = dp::lcs(std::string(""), std::string("ABC"));
    REQUIRE_EQ(r.value, 0LL);
}

TEST_CASE("LCS — single char match") {
    auto r = dp::lcs(std::string("A"), std::string("A"));
    REQUIRE_EQ(r.value, 1LL);
}

TEST_CASE("LCS — int vector version") {
    std::vector<int> a = {1,2,3,4,5};
    std::vector<int> b = {2,4,5,6};
    auto r = dp::lcs(a, b);
    REQUIRE_EQ(r.value, 3LL); // {2,4,5}
    // Verify indices in A form increasing subsequence matching B
    for (size_t i = 0; i < r.solution.size(); ++i)
        REQUIRE_EQ(a[r.solution[i]], b[/* matching index */ r.solution[i] >= 0 ? i : 0]);
    // Simpler: just check indices are increasing and values match
    for (size_t i = 1; i < r.solution.size(); ++i)
        REQUIRE(r.solution[i] > r.solution[i-1]);
    for (size_t i = 0; i < r.solution.size(); ++i)
        REQUIRE_EQ(a[r.solution[i]], b[i]); // relies on matching order
}

TEST_CASE("LCS — reconstruction yields valid indices") {
    auto r = dp::lcs(std::string("ABCBDAB"), std::string("BDCABA"));
    std::string a = "ABCBDAB";
    // Indices must be strictly increasing and in range
    for (int idx : r.solution) {
        REQUIRE(idx >= 0);
        REQUIRE(idx < (int)a.size());
    }
    for (size_t i = 1; i < r.solution.size(); ++i)
        REQUIRE(r.solution[i] > r.solution[i-1]);
}
