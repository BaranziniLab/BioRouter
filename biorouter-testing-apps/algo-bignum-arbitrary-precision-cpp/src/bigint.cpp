// bigint.cpp — Core construction, normalization, sign helpers

#include "bigint.hpp"
#include <limits>
#include <algorithm>

namespace bigint {

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

BigInt::BigInt() : negative_(false) {}

BigInt::BigInt(int64_t val) {
    if (val == 0) { negative_ = false; return; }
    negative_ = (val < 0);
    uint64_t u = negative_ ? static_cast<uint64_t>(-(val + 1)) + 1 : static_cast<uint64_t>(val);
    while (u > 0) {
        limbs_.push_back(static_cast<uint32_t>(u & 0xFFFFFFFFu));
        u >>= 32;
    }
}

BigInt::BigInt(const std::string& s) {
    if (s.empty()) throw std::invalid_argument("empty string");
    // Check for hex prefix
    if (s.size() > 2 && s[0] == '0' && (s[1] == 'x' || s[1] == 'X')) {
        *this = from_hex_string(s);
    } else if (s.size() > 1 && s[0] == '-' && s.size() > 3 && s[1] == '0' && (s[2] == 'x' || s[2] == 'X')) {
        BigInt tmp = from_hex_string(s.substr(1));
        tmp.negative_ = true;
        tmp.normalize();
        *this = std::move(tmp);
    } else {
        *this = from_decimal_string(s);
    }
}

// ---------------------------------------------------------------------------
// Sign & predicates
// ---------------------------------------------------------------------------

bool BigInt::is_zero() const { return limbs_.empty(); }
bool BigInt::is_positive() const { return !limbs_.empty() && !negative_; }
bool BigInt::is_negative() const { return !limbs_.empty() && negative_; }
bool BigInt::is_even() const { return limbs_.empty() || (limbs_[0] & 1) == 0; }
bool BigInt::is_odd()  const { return !limbs_.empty() && (limbs_[0] & 1) == 1; }

int BigInt::sign() const {
    if (limbs_.empty()) return 0;
    return negative_ ? -1 : 1;
}

BigInt BigInt::abs() const {
    BigInt r = *this;
    r.negative_ = false;
    return r;
}

void BigInt::set_zero() {
    limbs_.clear();
    negative_ = false;
}

void BigInt::normalize() {
    while (!limbs_.empty() && limbs_.back() == 0)
        limbs_.pop_back();
    if (limbs_.empty()) negative_ = false;
}

int BigInt::bit_length() const {
    if (is_zero()) return 0;
    uint32_t top = limbs_.back();
    int bits = static_cast<int>(limbs_.size() - 1) * 32;
    while (top > 0) { ++bits; top >>= 1; }
    return bits;
}

// ---------------------------------------------------------------------------
// Unsigned magnitude helpers
// ---------------------------------------------------------------------------

int BigInt::ucmp(const std::vector<uint32_t>& a, const std::vector<uint32_t>& b) {
    if (a.size() != b.size())
        return a.size() < b.size() ? -1 : 1;
    for (int i = static_cast<int>(a.size()) - 1; i >= 0; --i) {
        if (a[i] != b[i])
            return a[i] < b[i] ? -1 : 1;
    }
    return 0;
}

std::vector<uint32_t> BigInt::uadd(const std::vector<uint32_t>& a, const std::vector<uint32_t>& b) {
    size_t n = std::max(a.size(), b.size());
    std::vector<uint32_t> r(n);
    uint64_t carry = 0;
    for (size_t i = 0; i < n; ++i) {
        uint64_t av = i < a.size() ? a[i] : 0;
        uint64_t bv = i < b.size() ? b[i] : 0;
        uint64_t sum = av + bv + carry;
        r[i] = static_cast<uint32_t>(sum & 0xFFFFFFFFu);
        carry = sum >> 32;
    }
    if (carry) r.push_back(static_cast<uint32_t>(carry));
    return r;
}

std::vector<uint32_t> BigInt::usub(const std::vector<uint32_t>& a, const std::vector<uint32_t>& b) {
    // Assumes a >= b
    std::vector<uint32_t> r(a.size());
    uint64_t borrow = 0;
    for (size_t i = 0; i < a.size(); ++i) {
        uint64_t bv = i < b.size() ? b[i] : 0;
        uint64_t diff = static_cast<uint64_t>(a[i]) - bv - borrow;
        if (diff >> 63) { // underflow wrapped around
            r[i] = static_cast<uint32_t>(diff & 0xFFFFFFFFu);
            borrow = 1;
        } else {
            r[i] = static_cast<uint32_t>(diff);
            borrow = 0;
        }
    }
    return r;
}

std::vector<uint32_t> BigInt::umul_single(const std::vector<uint32_t>& a, uint32_t b) {
    if (b == 0 || a.empty()) return {};
    std::vector<uint32_t> r(a.size());
    uint64_t carry = 0;
    for (size_t i = 0; i < a.size(); ++i) {
        uint64_t prod = static_cast<uint64_t>(a[i]) * b + carry;
        r[i] = static_cast<uint32_t>(prod & 0xFFFFFFFFu);
        carry = prod >> 32;
    }
    if (carry) r.push_back(static_cast<uint32_t>(carry));
    return r;
}

void BigInt::uadd_shifted(std::vector<uint32_t>& a, const std::vector<uint32_t>& b, size_t shift) {
    if (b.empty()) return;
    if (a.size() < shift + b.size()) a.resize(shift + b.size(), 0);
    uint64_t carry = 0;
    for (size_t i = 0; i < b.size() || carry; ++i) {
        size_t idx = shift + i;
        if (idx >= a.size()) a.push_back(0);
        uint64_t bv = i < b.size() ? b[i] : 0;
        uint64_t sum = static_cast<uint64_t>(a[idx]) + bv + carry;
        a[idx] = static_cast<uint32_t>(sum & 0xFFFFFFFFu);
        carry = sum >> 32;
    }
}

} // namespace bigint
