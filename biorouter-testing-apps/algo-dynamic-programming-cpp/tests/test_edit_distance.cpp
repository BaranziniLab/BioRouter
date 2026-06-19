#include "test_framework.hpp"
#include "dp/edit_distance.hpp"

TEST_CASE("Edit Distance — basic") {
    auto r = dp::edit_distance(std::string("kitten"), std::string("sitting"));
    REQUIRE_EQ(r.value, 3LL);
}

TEST_CASE("Edit Distance — identical strings") {
    auto r = dp::edit_distance(std::string("abc"), std::string("abc"));
    REQUIRE_EQ(r.value, 0LL);
    REQUIRE(r.solution.empty());
}

TEST_CASE("Edit Distance — empty source") {
    auto r = dp::edit_distance(std::string(""), std::string("abc"));
    REQUIRE_EQ(r.value, 3LL);
    // All inserts
    for (int op : r.solution) REQUIRE_EQ(op, 2);
}

TEST_CASE("Edit Distance — empty target") {
    auto r = dp::edit_distance(std::string("abc"), std::string(""));
    REQUIRE_EQ(r.value, 3LL);
    for (int op : r.solution) REQUIRE_EQ(op, 3);
}

TEST_CASE("Edit Distance — both empty") {
    auto r = dp::edit_distance(std::string(""), std::string(""));
    REQUIRE_EQ(r.value, 0LL);
}

TEST_CASE("Edit Distance — single char replace") {
    auto r = dp::edit_distance(std::string("a"), std::string("b"));
    REQUIRE_EQ(r.value, 1LL);
    REQUIRE_EQ(r.solution.size(), 1u);
}

TEST_CASE("Edit Distance — int vector version") {
    std::vector<int> a = {1,2,3};
    std::vector<int> b = {1,4,3};
    auto r = dp::edit_distance(a, b);
    REQUIRE_EQ(r.value, 1LL);
}

TEST_CASE("Edit Distance — reconstruction ops length") {
    auto r = dp::edit_distance(std::string("sunday"), std::string("saturday"));
    REQUIRE_EQ(r.value, 3LL);
    // ops should reconstruct the transformation
}
