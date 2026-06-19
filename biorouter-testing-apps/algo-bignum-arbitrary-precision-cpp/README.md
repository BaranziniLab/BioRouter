# BigInt — Arbitrary-Precision Integer Library (C++17)

A modern C++17 library for arbitrary-precision integer arithmetic, using sign-magnitude representation with base 2³² limbs.

## Features

- **Full arithmetic**: `+ - * / %`, unary `-`, increment/decrement, compound assignment
- **Comparison**: all six relational operators
- **Construction**: from `int64_t`, decimal strings, hex strings (`0x` prefix)
- **String conversion**: decimal (`to_string()`) and hexadecimal (`to_hex_string()`)
- **Fast multiplication**: schoolbook O(n²) for small operands, Karatsuba O(n^1.585) above a configurable threshold
- **Division**: Knuth's Algorithm D for multi-precision long division
- **Number theory**: `pow`, `modpow` (binary exponentiation), `gcd` (Euclidean)
- **Literal syntax**: `"12345"_bi` user-defined literal

## Building

```bash
cmake -S . -B build
cmake --build build
```

## Running

```bash
# Tests
./build/bigint_tests

# Benchmarks (factorial, fibonacci, modpow)
./build/bigint_bench

# Interactive calculator
./build/bigint_cli
```

## Project Structure

```
include/
  bigint.hpp          — BigInt class declaration
  test_framework.hpp  — Assertion-based test macros
src/
  bigint.cpp              — Core construction, normalization, sign handling
  bigint_arithmetic.cpp   — Addition, subtraction, multiplication dispatch
  bigint_comparison.cpp   — All comparison operators, stream output
  bigint_division.cpp     — Knuth's Algorithm D division/modulo
  bigint_karatsuba.cpp    — Karatsuba fast multiplication
  bigint_math.cpp         — pow, modpow, gcd
  bigint_string.cpp       — String parsing and formatting (decimal + hex)
tests/
  test_main.cpp           — Test runner
  test_construct.cpp      — Construction and parsing tests
  test_arithmetic.cpp     — Arithmetic operation tests
  test_comparison.cpp     — Comparison operator tests
  test_division.cpp       — Division/modulo edge-case tests
  test_karatsuba.cpp      — Karatsuba vs schoolbook agreement
  test_math.cpp           — pow, modpow, gcd tests
  test_signs.cpp          — Sign edge-case tests
  test_string.cpp         — String round-trip tests
bench/
  bench_main.cpp          — Factorial, fibonacci, modpow benchmarks
cli/
  cli_main.cpp            — Expression calculator with +,-,*,/,%,pow,gcd
```

## Internal Representation

Each `BigInt` stores:
- `std::vector<uint32_t> limbs_` — magnitude in little-endian base 2³²
- `bool negative_` — sign flag (zero is always non-negative)

The Karatsuba threshold is 32 limbs (1024 bits).

## License

MIT
