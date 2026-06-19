#include "dp/coin_change.hpp"
#include <algorithm>

namespace dp {

DpResult coin_change_min(const std::vector<int>& coins, int amount) {
    if (amount < 0) return {-1, {}, {}};
    if (amount == 0) return {0, {}, {}};

    std::vector<long long> dp(static_cast<size_t>(amount) + 1, INF);
    std::vector<int> last(static_cast<size_t>(amount) + 1, -1);
    dp[0] = 0;

    for (int a = 1; a <= amount; ++a) {
        for (int c : coins) {
            if (c <= a && dp[a - c] + 1 < dp[a]) {
                dp[a] = dp[a - c] + 1;
                last[a] = c;
            }
        }
    }

    if (dp[amount] >= INF) return {-1, {}, {}};

    // Reconstruction
    std::vector<int> used;
    int a = amount;
    while (a > 0) {
        used.push_back(last[a]);
        a -= last[a];
    }
    return {dp[amount], used, {}};
}

DpResult coin_change_count(const std::vector<int>& coins, int amount) {
    if (amount < 0) return {0, {}, {}};
    if (amount == 0) return {1, {}, {}};

    std::vector<long long> dp(static_cast<size_t>(amount) + 1, 0);
    dp[0] = 1;

    for (int c : coins)
        for (int a = c; a <= amount; ++a)
            dp[a] += dp[a - c];

    return {dp[amount], {}, {}};
}

} // namespace dp
