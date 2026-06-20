// bigint_comparison.cpp — All comparison operators

#include "bigint.hpp"

namespace bigint {

bool BigInt::operator==(const BigInt& o) const {
    if (is_zero() && o.is_zero()) return true;
    if (negative_ != o.negative_) return false;
    return limbs_ == o.limbs_;
}

bool BigInt::operator!=(const BigInt& o) const {
    return !(*this == o);
}

bool BigInt::operator<(const BigInt& o) const {
    if (is_zero() && o.is_zero()) return false;
    if (negative_ != o.negative_) return negative_;
    int cmp = ucmp(limbs_, o.limbs_);
    return negative_ ? (cmp > 0) : (cmp < 0);
}

bool BigInt::operator<=(const BigInt& o) const {
    return !(o < *this);
}

bool BigInt::operator>(const BigInt& o) const {
    return o < *this;
}

bool BigInt::operator>=(const BigInt& o) const {
    return !(*this < o);
}

std::ostream& operator<<(std::ostream& os, const BigInt& bi) {
    os << bi.to_string();
    return os;
}

} // namespace bigint
