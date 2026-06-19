// bigint_division.cpp — Division and modulo using Knuth's Algorithm D

#include "bigint.hpp"
#include <stdexcept>

namespace bigint {

// Knuth's Algorithm D for multi-precision division.
// Returns {quotient, remainder} for u_in / v_in (unsigned, non-empty).
std::pair<std::vector<uint32_t>, std::vector<uint32_t>>
BigInt::udivmod(const std::vector<uint32_t>& u_in, const std::vector<uint32_t>& v_in) {
    if (v_in.empty()) throw std::domain_error("division by zero");

    // -----------------------------------------------------------------
    // Single-limb divisor: simple O(n) long division
    // -----------------------------------------------------------------
    if (v_in.size() == 1) {
        const uint64_t d = v_in[0];
        std::vector<uint32_t> q;
        q.reserve(u_in.size());
        uint64_t rem = 0;
        for (int i = static_cast<int>(u_in.size()) - 1; i >= 0; --i) {
            uint64_t cur = (rem << 32) | u_in[i];
            q.push_back(static_cast<uint32_t>(cur / d));
            rem = cur % d;
        }
        std::reverse(q.begin(), q.end());
        while (!q.empty() && q.back() == 0) q.pop_back();
        std::vector<uint32_t> r;
        if (rem != 0) r.push_back(static_cast<uint32_t>(rem));
        return {q, r};
    }

    // -----------------------------------------------------------------
    // Multi-limb divisor: Knuth Algorithm D
    // -----------------------------------------------------------------
    const size_t n = v_in.size();

    // If u < v, quotient is 0 and remainder is u.
    if (ucmp(u_in, v_in) < 0) {
        return {{}, u_in};
    }

    // --- Step D1: Normalize — left-shift so v[n-1] >= 2^31 ---
    const int shift = __builtin_clz(v_in[n - 1]);

    // u gets one extra leading zero limb to absorb the shift overflow.
    std::vector<uint32_t> u(u_in.size() + 1, 0);
    for (size_t i = 0; i < u_in.size(); ++i) u[i] = u_in[i];

    std::vector<uint32_t> v(v_in);  // same length as v_in

    if (shift > 0) {
        uint64_t carry = 0;
        for (size_t i = 0; i < u.size(); ++i) {
            uint64_t cur = (static_cast<uint64_t>(u[i]) << shift) | carry;
            u[i] = static_cast<uint32_t>(cur);
            carry = cur >> 32;
        }
        carry = 0;
        for (size_t i = 0; i < v.size(); ++i) {
            uint64_t cur = (static_cast<uint64_t>(v[i]) << shift) | carry;
            v[i] = static_cast<uint32_t>(cur);
            carry = cur >> 32;
        }
    }

    // m = number of quotient limbs minus 1
    const int m = static_cast<int>(u.size()) - 1 - static_cast<int>(n);
    std::vector<uint32_t> q(m + 1, 0);

    const uint64_t v_hi = v[n - 1];
    const uint64_t v_lo = (n >= 2) ? static_cast<uint64_t>(v[n - 2]) : 0;

    // --- Steps D2–D7: main loop ---
    for (int j = m; j >= 0; --j) {

        // --- Step D3: Trial quotient q̂ ---
        const uint64_t u_hi = u[j + n];
        const uint64_t u_lo = u[j + n - 1];

        uint64_t qhat, rhat;
        if (u_hi >= v_hi) {
            qhat = 0xFFFFFFFFu;
            // r̂ = (u_hi·2³² + u_lo) − q̂·v_hi
            //     Both terms fit in 64 bits; the difference is ≥ 0 because
            //     u_hi ≥ v_hi  ⇒  u_hi·2³² + u_lo ≥ v_hi·2³²
            //     and q̂·v_hi = (2³²−1)·v_hi < v_hi·2³²  when v_hi > 0.
            uint64_t window = (u_hi << 32) + u_lo;
            rhat = window - qhat * v_hi;
        } else {
            uint64_t window = (u_hi << 32) + u_lo;
            qhat = window / v_hi;
            rhat = window % v_hi;
        }

        // Refine: while  q̂·v_{n−2} > r̂·2³² + u_{j+n−2}  (Knuth Step D3, test)
        while (true) {
            // 128-bit-ish comparison via two 64-bit limbs:
            //   lhs = q̂ * v_lo   (fits in 64 bits since both < 2³²)
            //   rhs = rhat * 2³² + u[j+n-2]
            uint64_t lhs = qhat * v_lo;
            uint64_t rhs_lo = (j + static_cast<int>(n) - 2 >= 0)
                                ? static_cast<uint64_t>(u[j + n - 2]) : 0;
            // rhat < 2³² after normalisation, so (rhat << 32) fits in 64 bits
            uint64_t rhs = (rhat << 32) + rhs_lo;
            if (lhs <= rhs) break;
            --qhat;
            rhat += v_hi;
            if (rhat >= (1ULL << 32)) break;  // r̂ overflowed 32 bits ⇒ done
        }

        // --- Step D4: Multiply and subtract  u[j..j+n] −= q̂·v ---
        // Uses a running carry for the multiply, and int64_t sub_borrow
        // for the subtract (borrow = −1, 0).
        uint64_t mul_carry = 0;
        int64_t  sub_borrow = 0;
        for (size_t i = 0; i < n; ++i) {
            uint64_t p   = qhat * static_cast<uint64_t>(v[i]) + mul_carry;
            mul_carry    = p >> 32;
            uint32_t plo = static_cast<uint32_t>(p);

            int64_t diff = static_cast<int64_t>(static_cast<uint64_t>(u[j + i]))
                         - static_cast<int64_t>(static_cast<uint64_t>(plo))
                         + sub_borrow;
            u[j + i]    = static_cast<uint32_t>(static_cast<uint64_t>(diff));
            sub_borrow  = diff >> 32;   // −1 on borrow, 0 otherwise
        }
        // Final limb: subtract the multiply carry
        int64_t diff = static_cast<int64_t>(static_cast<uint64_t>(u[j + n]))
                     - static_cast<int64_t>(mul_carry)
                     + sub_borrow;
        u[j + n]      = static_cast<uint32_t>(static_cast<uint64_t>(diff));
        int64_t final_borrow = diff >> 32;

        q[j] = static_cast<uint32_t>(qhat);

        // --- Step D6: Add back (extremely rare — at most once per iteration) ---
        if (final_borrow != 0) {
            --q[j];
            uint64_t add_c = 0;
            for (size_t i = 0; i < n; ++i) {
                uint64_t sum = static_cast<uint64_t>(u[j + i])
                             + static_cast<uint64_t>(v[i]) + add_c;
                u[j + i] = static_cast<uint32_t>(sum);
                add_c = sum >> 32;
            }
            u[j + n] = static_cast<uint32_t>(
                static_cast<uint64_t>(u[j + n]) + add_c);
        }
    }

    // --- Step D8: Un-shift remainder ---
    std::vector<uint32_t> r(n);
    if (shift > 0) {
        for (size_t i = 0; i < n - 1; ++i)
            r[i] = (u[i] >> shift) | (u[i + 1] << (32 - shift));
        r[n - 1] = u[n - 1] >> shift;
    } else {
        for (size_t i = 0; i < n; ++i) r[i] = u[i];
    }

    // Strip leading zeros
    while (!q.empty() && q.back() == 0) q.pop_back();
    while (!r.empty() && r.back() == 0) r.pop_back();

    return {q, r};
}

// ---------------------------------------------------------------------------
BigInt BigInt::operator/(const BigInt& o) const {
    if (o.is_zero()) throw std::domain_error("division by zero");
    if (is_zero()) return BigInt();
    auto [q, _] = udivmod(limbs_, o.limbs_);
    BigInt result;
    result.limbs_ = std::move(q);
    result.negative_ = (negative_ != o.negative_);
    result.normalize();
    return result;
}

BigInt BigInt::operator%(const BigInt& o) const {
    if (o.is_zero()) throw std::domain_error("modulo by zero");
    if (is_zero()) return BigInt();
    auto [_, r] = udivmod(limbs_, o.limbs_);
    BigInt result;
    result.limbs_ = std::move(r);
    result.negative_ = negative_;
    result.normalize();
    return result;
}

BigInt& BigInt::operator/=(const BigInt& o) { *this = *this / o; return *this; }
BigInt& BigInt::operator%=(const BigInt& o) { *this = *this % o; return *this; }

} // namespace bigint
