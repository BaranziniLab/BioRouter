cmake_minimum_required(VERSION 3.14)
project(bio-kmer-counter
    VERSION 1.0.0
    DESCRIPTION "A k-mer counting and de Bruijn graph toolkit in modern C++17"
    LANGUAGES CXX
)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# --- Compiler warnings ---
if(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
    add_compile_options(-Wall -Wextra -Wpedantic)
endif()

# --- Library (shared between CLI and tests) ---
add_library(bkc_core STATIC
    src/kmer.cpp
    src/counter.cpp
    src/dbg.cpp
    src/io.cpp
    src/cli.cpp
)
target_include_directories(bkc_core PUBLIC src)

# --- CLI executable ---
add_executable(bio-kmer-counter src/main.cpp)
target_link_libraries(bio-kmer-counter PRIVATE bkc_core)

# --- Test suite ---
enable_testing()

# Collect all test source files.
set(TEST_SOURCES
    tests/test_main.cpp
    tests/test_kmer.cpp
    tests/test_counter.cpp
    tests/test_dbg.cpp
    tests/test_io.cpp
)

add_executable(bkc_tests ${TEST_SOURCES})
target_link_libraries(bkc_tests PRIVATE bkc_core)

add_test(
    NAME unit_tests
    COMMAND bkc_tests
)

# --- Benchmark ---
add_executable(bkc_benchmark benchmarks/benchmark_kmer.cpp)
target_link_libraries(bkc_benchmark PRIVATE bkc_core)
