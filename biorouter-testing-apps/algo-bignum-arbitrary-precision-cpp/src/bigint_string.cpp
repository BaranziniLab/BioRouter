// bigint_string.cpp — String conversion and parsing

#include "bigint.hpp"
#include <sstream>
#include <iomanip>
#include <algorithm>

namespace bigint {

// --- Decimal to string ---
std::string BigInt::to_string() const {
    if (is_zero()) return "0";

    // Repeated division by 10
    std::vector<uint32_t> tmp = limbs_;
    std::string result;

    while (!tmp.empty()) {
        uint64_t remainder = 0;
        for (int i = static_cast<int>(tmp.size()) - 1; i >= 0; --i) {
            uint64_t cur = (remainder << 32) | tmp[i];
            tmp[i] = static_cast<uint32_t>(cur / 10);
            remainder = cur % 10;
        }
        result.push_back('0' + static_cast<char>(remainder));
        while (!tmp.empty() && tmp.back() == 0) tmp.pop_back();
    }

    if (negative_) result.push_back('-');
    std::reverse(result.begin(), result.end());
    return result;
}

// --- Hex to string ---
std::string BigInt::to_hex_string() const {
    if (is_zero()) return "0";

    std::ostringstream oss;
    if (negative_) oss << '-';
    oss << std::hex;

    // Print most significant limb without leading zeros
    bool first = true;
    for (int i = static_cast<int>(limbs_.size()) - 1; i >= 0; --i) {
        if (first) {
            oss << limbs_[i];
            first = false;
        } else {
            oss << std::setfill('0') << std::setw(8) << limbs_[i];
        }
    }
    return oss.str();
}

// --- Parse decimal string ---
BigInt BigInt::from_decimal_string(const std::string& s) {
    if (s.empty()) throw std::invalid_argument("empty string");

    size_t start = 0;
    bool neg = false;
    if (s[0] == '-') { neg = true; start = 1; }
    else if (s[0] == '+') { start = 1; }

    if (start >= s.size()) throw std::invalid_argument("invalid number");

    BigInt result;
    for (size_t i = start; i < s.size(); ++i) {
        char c = s[i];
        if (c < '0' || c > '9') throw std::invalid_argument(std::string("invalid digit: ") + c);
        // result = result * 10 + digit
        result = result * BigInt(10) + BigInt(static_cast<int64_t>(c - '0'));
    }

    result.negative_ = neg;
    result.normalize();
    return result;
}

// --- Parse hex string (without "0x" prefix) ---
BigInt BigInt::from_hex_string(const std::string& s_full) {
    std::string s = s_full;
    // Strip "0x" prefix if present
    if (s.size() > 2 && s[0] == '0' && (s[1] == 'x' || s[1] == 'X'))
        s = s.substr(2);

    if (s.empty()) throw std::invalid_argument("empty hex string");

    bool neg = false;
    if (s[0] == '-') { neg = true; s = s.substr(1); }

    BigInt result;
    // Process 8 hex digits at a time (one uint32_t limb)
    size_t i = s.size();
    while (i > 0) {
        size_t chunk = std::min(i, size_t(8));
        std::string sub = s.substr(i - chunk, chunk);
        uint32_t limb = 0;
        for (char c : sub) {
            limb <<= 4;
            if (c >= '0' && c <= '9') limb |= (c - '0');
            else if (c >= 'a' && c <= 'f') limb |= (c - 'a' + 10);
            else if (c >= 'A' && c <= 'F') limb |= (c - 'A' + 10);
            else throw std::invalid_argument(std::string("invalid hex digit: ") + c);
        }
        result.limbs_.push_back(limb);
        i -= chunk;
    }

    result.negative_ = neg;
    result.normalize();
    return result;
}

// --- User-defined literal ---
BigInt operator""_bi(const char* s, size_t) {
    return BigInt(s);
}

} // namespace bigint
