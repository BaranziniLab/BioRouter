// bigint_arithmetic.cpp — + - * unary-, increment/decrement

#include "bigint.hpp"

namespace bigint {

// ---------------------------------------------------------------------------
// Addition
// ---------------------------------------------------------------------------

BigInt BigInt::operator+(const BigInt& o) const {
    if (negative_ == o.negative_) {
        // Same sign: add magnitudes, keep sign
        BigInt r;
        r.limbs_ = uadd(limbs_, o.limbs_);
        r.negative_ = negative_;
        r.normalize();
        return r;
    }
    // Different signs: subtract smaller magnitude from larger
    int cmp = ucmp(limbs_, o.limbs_);
    if (cmp == 0) return BigInt(); // zero
    BigInt r;
    if (cmp > 0) {
        r.limbs_ = usub(limbs_, o.limbs_);
        r.negative_ = negative_;
    } else {
        r.limbs_ = usub(o.limbs_, limbs_);
        r.negative_ = o.negative_;
    }
    r.normalize();
    return r;
}

// ---------------------------------------------------------------------------
// Subtraction
// ---------------------------------------------------------------------------

BigInt BigInt::operator-(const BigInt& o) const {
    BigInt neg_o = o;
    neg_o.negative_ = !neg_o.negative_;
    if (neg_o.is_zero()) neg_o.negative_ = false;
    return *this + neg_o;
}

// ---------------------------------------------------------------------------
// Multiplication (dispatch to schoolbook or Karatsuba)
// ---------------------------------------------------------------------------

BigInt BigInt::operator*(const BigInt& o) const {
    if (is_zero() || o.is_zero()) return BigInt();
    BigInt r;
    if (limbs_.size() < KARATSUBA_THRESHOLD || o.limbs_.size() < KARATSUBA_THRESHOLD) {
        r.limbs_ = umul_schoolbook(limbs_, o.limbs_);
    } else {
        r.limbs_ = umul_karatsuba(limbs_, o.limbs_);
    }
    r.negative_ = (negative_ != o.negative_);
    r.normalize();
    return r;
}

// Schoolbook multiplication O(n*m)
std::vector<uint32_t> BigInt::umul_schoolbook(const std::vector<uint32_t>& a, const std::vector<uint32_t>& b) {
    if (a.empty() || b.empty()) return {};
    std::vector<uint32_t> r(a.size() + b.size(), 0);
    for (size_t i = 0; i < a.size(); ++i) {
        if (a[i] == 0) continue;
        uint64_t carry = 0;
        for (size_t j = 0; j < b.size() || carry; ++j) {
            uint64_t cur = static_cast<uint64_t>(r[i + j])
                         + static_cast<uint64_t>(a[i]) * (j < b.size() ? b[j] : 0)
                         + carry;
            r[i + j] = static_cast<uint32_t>(cur & 0xFFFFFFFFu);
            carry = cur >> 32;
        }
    }
    // Remove trailing zeros handled by normalize
    while (!r.empty() && r.back() == 0) r.pop_back();
    return r;
}

// Unary minus
BigInt BigInt::operator-() const {
    if (is_zero()) return *this;
    BigInt r = *this;
    r.negative_ = !r.negative_;
    return r;
}

// Increment / Decrement
BigInt& BigInt::operator++() {
    *this = *this + BigInt(1);
    return *this;
}
BigInt BigInt::operator++(int) {
    BigInt tmp = *this;
    ++(*this);
    return tmp;
}
BigInt& BigInt::operator--() {
    *this = *this - BigInt(1);
    return *this;
}
BigInt BigInt::operator--(int) {
    BigInt tmp = *this;
    --(*this);
    return tmp;
}

// Compound assignment
BigInt& BigInt::operator+=(const BigInt& o) { *this = *this + o; return *this; }
BigInt& BigInt::operator-=(const BigInt& o) { *this = *this - o; return *this; }
BigInt& BigInt::operator*=(const BigInt& o) { *this = *this * o; return *this; }

} // namespace bigint
