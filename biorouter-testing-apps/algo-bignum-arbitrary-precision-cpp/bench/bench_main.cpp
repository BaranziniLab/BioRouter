// bench_main.cpp — Benchmarks: factorial, fibonacci, modpow
#include "bigint.hpp"
#include <iostream>
#include <chrono>

using namespace bigint;
using Clock = std::chrono::high_resolution_clock;

static void bench_factorial(int n) {
    auto start = Clock::now();
    BigInt result(1);
    for (int i = 2; i <= n; ++i) {
        result = result * BigInt(i);
    }
    auto end = Clock::now();
    double ms = std::chrono::duration<double, std::milli>(end - start).count();
    std::string s = result.to_string();
    std::cout << "factorial(" << n << "): " << ms << " ms, "
              << s.size() << " decimal digits" << std::endl;
}

static void bench_fibonacci(int n) {
    auto start = Clock::now();
    BigInt a(0), b(1);
    for (int i = 0; i < n; ++i) {
        BigInt c = a + b;
        a = b;
        b = c;
    }
    auto end = Clock::now();
    double ms = std::chrono::duration<double, std::milli>(end - start).count();
    std::string s = b.to_string();
    std::cout << "fibonacci(" << n << "): " << ms << " ms, "
              << s.size() << " decimal digits" << std::endl;
}

static void bench_modpow(int bits) {
    // Compute 3^(2^bits-1) mod (2^bits + 1)
    BigInt base(3);
    BigInt exp = BigInt::pow(BigInt(2), bits) - BigInt(1);
    BigInt mod = BigInt::pow(BigInt(2), bits) + BigInt(1);

    auto start = Clock::now();
    BigInt result = BigInt::modpow(base, exp, mod);
    auto end = Clock::now();
    double ms = std::chrono::duration<double, std::milli>(end - start).count();
    std::cout << "modpow(3, 2^" << bits << "-1, 2^" << bits << "+1): "
              << ms << " ms" << std::endl;
}

int main() {
    std::cout << "=== BigInt Benchmarks ===\n\n";

    bench_factorial(100);
    bench_factorial(1000);
    bench_factorial(5000);
    bench_factorial(10000);

    std::cout << std::endl;

    bench_fibonacci(1000);
    bench_fibonacci(10000);
    bench_fibonacci(100000);
    bench_fibonacci(500000);

    std::cout << std::endl;

    bench_modpow(256);
    bench_modpow(512);
    bench_modpow(1024);
    bench_modpow(2048);
    bench_modpow(4096);

    std::cout << "\n=== Done ===\n";
    return 0;
}
