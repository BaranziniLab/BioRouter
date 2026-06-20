#include "dp/knapsack_unbounded.hpp"
#include <algorithm>

namespace dp {

DpResult knapsack_unbounded(const std::vector<int>& weights,
                            const std::vector<int>& values,
                            int capacity) {
    int n = static_cast<int>(weights.size());
    if (n == 0 || capacity <= 0)
        return {0, {}, {}};

    // dp[w] = best value for capacity w (unbounded items)
    std::vector<long long> dp(static_cast<size_t>(capacity) + 1, 0);
    // choice[w] = index of last item added for capacity w (-1 = none)
    std::vector<int> choice(static_cast<size_t>(capacity) + 1, -1);

    for (int w = 1; w <= capacity; ++w) {
        for (int i = 0; i < n; ++i) {
            if (weights[i] <= w) {
                long long cand = dp[w - weights[i]] + values[i];
                if (cand > dp[w]) {
                    dp[w] = cand;
                    choice[w] = i;
                }
            }
        }
    }

    // Reconstruction
    std::vector<int> chosen;
    int w = capacity;
    while (w > 0 && choice[w] != -1) {
        chosen.push_back(choice[w]);
        w -= weights[choice[w]];
    }
    return {dp[capacity], chosen, {}};
}

} // namespace dp
