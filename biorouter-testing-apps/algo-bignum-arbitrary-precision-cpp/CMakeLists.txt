cmake_minimum_required(VERSION 3.14)
project(bigint LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# --- Library ---
add_library(bigint STATIC
    src/bigint.cpp
    src/bigint_arithmetic.cpp
    src/bigint_division.cpp
    src/bigint_comparison.cpp
    src/bigint_string.cpp
    src/bigint_math.cpp
    src/bigint_karatsuba.cpp
)
target_include_directories(bigint PUBLIC include)

# --- Tests ---
add_executable(bigint_tests
    tests/test_main.cpp
    tests/test_construct.cpp
    tests/test_arithmetic.cpp
    tests/test_comparison.cpp
    tests/test_division.cpp
    tests/test_string.cpp
    tests/test_karatsuba.cpp
    tests/test_math.cpp
    tests/test_signs.cpp
)
target_link_libraries(bigint_tests PRIVATE bigint)

# --- Benchmarks ---
add_executable(bigint_bench bench/bench_main.cpp)
target_link_libraries(bigint_bench PRIVATE bigint)

# --- CLI Calculator ---
add_executable(bigint_cli cli/cli_main.cpp)
target_link_libraries(bigint_cli PRIVATE bigint)
