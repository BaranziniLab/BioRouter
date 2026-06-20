cmake_minimum_required(VERSION 3.14)
project(algo-dynamic-programming-cpp VERSION 1.0.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# ── Library ──────────────────────────────────────────────────────────
add_library(dp_solvers STATIC
    src/solvers/knapsack_01.cpp
    src/solvers/knapsack_unbounded.cpp
    src/solvers/lcs.cpp
    src/solvers/edit_distance.cpp
    src/solvers/lis.cpp
    src/solvers/matrix_chain.cpp
    src/solvers/coin_change.cpp
    src/solvers/rod_cutting.cpp
    src/solvers/subset_sum.cpp
    src/solvers/weighted_interval.cpp
    src/solvers/grid_min_path.cpp
)
target_include_directories(dp_solvers PUBLIC ${CMAKE_SOURCE_DIR}/include)

# ── Tests ────────────────────────────────────────────────────────────
add_executable(dp_tests
    tests/test_main.cpp
    tests/test_knapsack.cpp
    tests/test_knapsack_unbounded.cpp
    tests/test_lcs.cpp
    tests/test_edit_distance.cpp
    tests/test_lis.cpp
    tests/test_matrix_chain.cpp
    tests/test_coin_change.cpp
    tests/test_rod_cutting.cpp
    tests/test_subset_sum.cpp
    tests/test_weighted_interval.cpp
    tests/test_grid_min_path.cpp
)
target_link_libraries(dp_tests PRIVATE dp_solvers)
target_include_directories(dp_tests PRIVATE ${CMAKE_SOURCE_DIR}/include)

# ── CTest ────────────────────────────────────────────────────────────
enable_testing()
add_test(NAME dp_unit_tests COMMAND dp_tests)
