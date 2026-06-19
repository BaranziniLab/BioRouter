#include "dp/knapsack_01.hpp"
#include <algorithm>

namespace dp {

DpResult knapsack_01(const std::vector<int>& weights,
                     const std::vector<int>& values,
                     int capacity) {
    int n = static_cast<int>(weights.size());
    if (n == 0 || capacity <= 0)
        return {0, {}, {}};

    // dp[i][w] = best value using items 0..i-1 with capacity w
    std::vector<std::vector<long long>> dp(n + 1,
        std::vector<long long>(static_cast<size_t>(capacity) + 1, 0));

    for (int i = 1; i <= n; ++i) {
        int w_i = weights[i - 1];
        long long v_i = values[i - 1];
        for (int w = 0; w <= capacity; ++w) {
            dp[i][w] = dp[i - 1][w];
            if (w_i <= w)
                dp[i][w] = std::max(dp[i][w], dp[i - 1][w - w_i] + v_i);
        }
    }

    // Reconstruction
    std::vector<int> chosen;
    int w = capacity;
    for (int i = n; i >= 1; --i) {
        if (dp[i][w] != dp[i - 1][w]) {
            chosen.push_back(i - 1); // 0-based index
            w -= weights[i - 1];
        }
    }
    std::reverse(chosen.begin(), chosen.end());
    return {dp[n][capacity], chosen, {}};
}

} // namespace dp
