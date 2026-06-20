#include "dp/rod_cutting.hpp"
#include <algorithm>

namespace dp {

DpResult rod_cutting(const std::vector<int>& prices) {
    int n = static_cast<int>(prices.size());
    if (n == 0) return {0, {}, {}};

    // dp[i] = max revenue for rod of length i
    std::vector<long long> dp(static_cast<size_t>(n) + 1, 0);
    std::vector<int> first(static_cast<size_t>(n) + 1, 0);

    for (int i = 1; i <= n; ++i) {
        for (int j = 1; j <= i; ++j) {
            long long cand = dp[i - j] + prices[j - 1];
            if (cand > dp[i]) {
                dp[i] = cand;
                first[i] = j;
            }
        }
    }

    // Reconstruction: piece lengths
    std::vector<int> pieces;
    int remaining = n;
    while (remaining > 0) {
        pieces.push_back(first[remaining]);
        remaining -= first[remaining];
    }
    return {dp[n], pieces, {}};
}

} // namespace dp
