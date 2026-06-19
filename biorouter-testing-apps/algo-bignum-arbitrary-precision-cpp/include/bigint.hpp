#pragma once

#include <cstdint>
#include <string>
#include <vector>
#include <iostream>
#include <stdexcept>
#include <algorithm>
#include <cassert>

namespace bigint {

class BigInt {
public:
    // --- Construction ---
    BigInt();                                  // 0
    BigInt(int64_t val);                       // from signed integer
    explicit BigInt(const std::string& s);     // from decimal or hex string ("0x..." prefix)
    BigInt(const BigInt& other) = default;
    BigInt(BigInt&& other) noexcept = default;
    BigInt& operator=(const BigInt& other) = default;
    BigInt& operator=(BigInt&& other) noexcept = default;

    // --- String conversion ---
    std::string to_string() const;             // decimal
    std::string to_hex_string() const;         // hex (lowercase, no prefix)

    // --- Sign & predicates ---
    bool is_zero() const;
    bool is_positive() const;                  // > 0
    bool is_negative() const;                  // < 0
    bool is_even() const;
    bool is_odd() const;
    int  sign() const;                         // -1, 0, +1
    BigInt abs() const;

    // --- Comparison ---
    bool operator==(const BigInt& o) const;
    bool operator!=(const BigInt& o) const;
    bool operator<(const BigInt& o) const;
    bool operator<=(const BigInt& o) const;
    bool operator>(const BigInt& o) const;
    bool operator>=(const BigInt& o) const;

    // --- Arithmetic ---
    BigInt operator+(const BigInt& o) const;
    BigInt operator-(const BigInt& o) const;
    BigInt operator*(const BigInt& o) const;
    BigInt operator/(const BigInt& o) const;
    BigInt operator%(const BigInt& o) const;

    BigInt& operator+=(const BigInt& o);
    BigInt& operator-=(const BigInt& o);
    BigInt& operator*=(const BigInt& o);
    BigInt& operator/=(const BigInt& o);
    BigInt& operator%=(const BigInt& o);

    // --- Unary ---
    BigInt operator-() const;
    BigInt& operator++();       // prefix
    BigInt  operator++(int);    // postfix
    BigInt& operator--();
    BigInt  operator--(int);

    // --- Bit operations (needed internally, also useful) ---
    int bit_length() const;                    // number of bits to represent

    // --- Math ---
    static BigInt pow(const BigInt& base, uint64_t exp);
    static BigInt modpow(const BigInt& base, const BigInt& exp, const BigInt& mod);
    static BigInt gcd(BigInt a, BigInt b);

    // --- Stream output ---
    friend std::ostream& operator<<(std::ostream& os, const BigInt& bi);

    // Internal access for tests
    const std::vector<uint32_t>& limbs() const { return limbs_; }

private:
    // Little-endian: limbs_[0] is least significant
    std::vector<uint32_t> limbs_;
    bool negative_;  // true if negative (zero is always non-negative)

    void normalize();  // strip leading zeros, fix sign of zero
    void set_zero();

    // Unsigned helpers (operate on magnitudes, assume non-negative)
    static int  ucmp(const std::vector<uint32_t>& a, const std::vector<uint32_t>& b);
    static std::vector<uint32_t> uadd(const std::vector<uint32_t>& a, const std::vector<uint32_t>& b);
    static std::vector<uint32_t> usub(const std::vector<uint32_t>& a, const std::vector<uint32_t>& b); // requires a >= b
    static std::vector<uint32_t> umul_schoolbook(const std::vector<uint32_t>& a, const std::vector<uint32_t>& b);
    static std::vector<uint32_t> umul_karatsuba(const std::vector<uint32_t>& a, const std::vector<uint32_t>& b);
    static std::pair<std::vector<uint32_t>, std::vector<uint32_t>>
            udivmod(const std::vector<uint32_t>& a, const std::vector<uint32_t>& b); // Knuth Algorithm D

    // Multiply by a single limb
    static std::vector<uint32_t> umul_single(const std::vector<uint32_t>& a, uint32_t b);
    // Add with shift (a + b * 2^(32*shift))
    static void uadd_shifted(std::vector<uint32_t>& a, const std::vector<uint32_t>& b, size_t shift);

    // Karatsuba threshold (in limbs)
    static constexpr size_t KARATSUBA_THRESHOLD = 32;

    // Parse helpers
    static BigInt from_decimal_string(const std::string& s);
    static BigInt from_hex_string(const std::string& s);
};

// --- Free functions ---
BigInt operator""_bi(const char* s, size_t);

} // namespace bigint
