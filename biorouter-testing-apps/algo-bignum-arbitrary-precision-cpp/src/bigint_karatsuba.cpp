// bigint_karatsuba.cpp — Karatsuba fast multiplication

#include "bigint.hpp"
#include <algorithm>

namespace bigint {

// Karatsuba multiplication O(n^1.585)
// Split a = a1*B^m + a0, b = b1*B^m + b0 where B = 2^32
// z0 = a0*b0
// z2 = a1*b1
// z1 = (a0+a1)*(b0+b1) - z2 - z0
// result = z2*B^(2m) + z1*B^m + z0
std::vector<uint32_t> BigInt::umul_karatsuba(const std::vector<uint32_t>& a, const std::vector<uint32_t>& b) {
    size_t n = std::max(a.size(), b.size());
    if (n < KARATSUBA_THRESHOLD) {
        return umul_schoolbook(a, b);
    }

    size_t m = n / 2;

    // Split a into a0 (low) and a1 (high)
    std::vector<uint32_t> a0(a.begin(), a.begin() + std::min(m, a.size()));
    std::vector<uint32_t> a1(a.size() > m ? a.begin() + m : a.end(), a.end());

    // Split b into b0 (low) and b1 (high)
    std::vector<uint32_t> b0(b.begin(), b.begin() + std::min(m, b.size()));
    std::vector<uint32_t> b1(b.size() > m ? b.begin() + m : b.end(), b.end());

    // z2 = a1 * b1
    std::vector<uint32_t> z2 = umul_karatsuba(a1, b1);

    // z0 = a0 * b0
    std::vector<uint32_t> z0 = umul_karatsuba(a0, b0);

    // z1 = (a0 + a1) * (b0 + b1) - z2 - z0
    std::vector<uint32_t> a0a1 = uadd(a0, a1);
    std::vector<uint32_t> b0b1 = uadd(b0, b1);
    std::vector<uint32_t> z1 = umul_karatsuba(a0a1, b0b1);

    // Subtract z2 and z0 from z1
    // z1 = z1 - z2 - z0  (z1 >= z2 + z0 always holds)
    if (ucmp(z1, z2) < 0) {
        // Shouldn't happen with non-negative inputs, but pad if needed
        z1.resize(z2.size() + 1, 0);
    }
    z1 = usub(z1, z2);
    if (ucmp(z1, z0) < 0) {
        z1.resize(z0.size() + 1, 0);
    }
    z1 = usub(z1, z0);

    // result = z2 << (2*m*32) + z1 << (m*32) + z0
    std::vector<uint32_t> result;
    result.reserve(z2.size() + 2 * m + 2);

    // Add z0
    result = z0;

    // Add z1 shifted by m limbs
    uadd_shifted(result, z1, m);

    // Add z2 shifted by 2m limbs
    uadd_shifted(result, z2, 2 * m);

    while (!result.empty() && result.back() == 0) result.pop_back();
    return result;
}

} // namespace bigint
