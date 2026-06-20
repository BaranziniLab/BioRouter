#include "test_framework.hpp"
#include "dp/grid_min_path.hpp"

TEST_CASE("Grid Min Path — basic") {
    std::vector<std::vector<int>> grid = {
        {1, 3, 1},
        {1, 5, 1},
        {4, 2, 1}
    };
    auto r = dp::grid_min_path(grid);
    REQUIRE_EQ(r.value, 7LL); // 1→3→1→1→1 = 7
}

TEST_CASE("Grid Min Path — single cell") {
    std::vector<std::vector<int>> grid = {{5}};
    auto r = dp::grid_min_path(grid);
    REQUIRE_EQ(r.value, 5LL);
    REQUIRE(r.solution.empty());
}

TEST_CASE("Grid Min Path — single row") {
    std::vector<std::vector<int>> grid = {{1,2,3,4}};
    auto r = dp::grid_min_path(grid);
    REQUIRE_EQ(r.value, 10LL);
    for (int m : r.solution) REQUIRE_EQ(m, 0); // all right
}

TEST_CASE("Grid Min Path — single column") {
    std::vector<std::vector<int>> grid = {{1},{2},{3}};
    auto r = dp::grid_min_path(grid);
    REQUIRE_EQ(r.value, 6LL);
    for (int m : r.solution) REQUIRE_EQ(m, 1); // all down
}

TEST_CASE("Grid Min Path — reconstruction has correct length") {
    std::vector<std::vector<int>> grid = {{1,2},{3,4}};
    auto r = dp::grid_min_path(grid);
    // 2×2 grid: 1 right + 1 down = 2 moves
    REQUIRE_EQ(r.solution.size(), 2u);
    REQUIRE_EQ(r.value, 7LL); // 1→2→4 or 1→3→4=8. min=7(1,2,4)
}

TEST_CASE("Grid Min Path — all zeros") {
    std::vector<std::vector<int>> grid = {{0,0,0},{0,0,0}};
    auto r = dp::grid_min_path(grid);
    REQUIRE_EQ(r.value, 0LL);
}

TEST_CASE("Grid Min Path — large grid") {
    // 3x3 all ones → path length 4 (4 cells) → value=4
    std::vector<std::vector<int>> grid = {{1,1,1},{1,1,1},{1,1,1}};
    auto r = dp::grid_min_path(grid);
    REQUIRE_EQ(r.value, 5LL); // 5 cells visited
}
