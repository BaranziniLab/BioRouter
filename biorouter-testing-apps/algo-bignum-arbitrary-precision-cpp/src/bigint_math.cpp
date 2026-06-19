// bigint_math.cpp — pow, modpow, gcd

#include "bigint.hpp"
#include <stdexcept>

namespace bigint {

// Fast exponentiation by squaring
BigInt BigInt::pow(const BigInt& base, uint64_t exp) {
    if (exp == 0) return BigInt(1);
    if (base.is_zero()) return BigInt();

    BigInt result(1);
    BigInt b = base;

    while (exp > 0) {
        if (exp & 1) result = result * b;
        b = b * b;
        exp >>= 1;
    }
    return result;
}

// Modular exponentiation: base^exp mod mod (binary method)
BigInt BigInt::modpow(const BigInt& base, const BigInt& exp, const BigInt& mod) {
    if (mod.is_zero()) throw std::domain_error("modpow: modulus is zero");
    if (mod == BigInt(1)) return BigInt();
    if (exp.is_zero()) return BigInt(1);
    if (base.is_zero()) return BigInt();

    BigInt result(1);
    BigInt b = base % mod;
    // Make sure b is non-negative
    if (b.is_negative()) b = b + mod;

    BigInt e = exp;
    while (!e.is_zero()) {
        // Check if e is odd
        if (e.is_odd()) {
            result = (result * b) % mod;
            if (result.is_negative()) result = result + mod;
        }
        e = e / BigInt(2);
        b = (b * b) % mod;
        if (b.is_negative()) b = b + mod;
    }
    return result;
}

// Euclidean GCD
BigInt BigInt::gcd(BigInt a, BigInt b) {
    a = a.abs();
    b = b.abs();
    while (!b.is_zero()) {
        BigInt t = a % b;
        a = std::move(b);
        b = std::move(t);
    }
    return a;
}

} // namespace bigint
