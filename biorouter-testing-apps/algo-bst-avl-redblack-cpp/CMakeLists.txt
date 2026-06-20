cmake_minimum_required(VERSION 3.14)
project(algo-bst-avl-redblack-cpp LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# ── Header-only library ──────────────────────────────────────────────
add_library(bst_lib INTERFACE)
target_include_directories(bst_lib INTERFACE
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
    $<INSTALL_INTERFACE:include>
)

# ── Tests ────────────────────────────────────────────────────────────
add_executable(tests
    tests/test_main.cpp
    tests/test_bst.cpp
    tests/test_avl.cpp
    tests/test_rbtree.cpp
    tests/test_stress.cpp
)
target_link_libraries(tests PRIVATE bst_lib)

# ── Benchmark ────────────────────────────────────────────────────────
add_executable(benchmark bench/benchmark.cpp)
target_link_libraries(benchmark PRIVATE bst_lib)
