#include "dp/matrix_chain.hpp"
#include <functional>

namespace dp {

DpResult matrix_chain(const std::vector<int>& dims) {
    int n = static_cast<int>(dims.size()) - 1; // number of matrices
    if (n <= 0) return {0, {}, {}};
    if (n == 1) return {0, {}, {}};

    // dp[i][j] = min cost to multiply matrices i..j (0-based)
    // split[i][j] = optimal split point k for matrices i..j
    std::vector<std::vector<long long>> dp(static_cast<size_t>(n),
        std::vector<long long>(static_cast<size_t>(n), 0));
    std::vector<std::vector<int>> split(static_cast<size_t>(n),
        std::vector<int>(static_cast<size_t>(n), 0));

    // chain length L
    for (int L = 2; L <= n; ++L) {
        for (int i = 0; i <= n - L; ++i) {
            int j = i + L - 1;
            dp[i][j] = INF;
            for (int k = i; k < j; ++k) {
                long long cost = dp[i][k] + dp[k + 1][j]
                    + static_cast<long long>(dims[i]) * dims[k + 1] * dims[j + 1];
                if (cost < dp[i][j]) {
                    dp[i][j] = cost;
                    split[i][j] = k;
                }
            }
        }
    }

    // Reconstruct split points into a flat list (preorder traversal of parenthesization tree)
    std::vector<int> splits;
    std::function<void(int,int)> collect = [&](int i, int j) {
        if (i >= j) return;
        splits.push_back(split[i][j]);
        collect(i, split[i][j]);
        collect(split[i][j] + 1, j);
    };
    collect(0, n - 1);

    return {dp[0][n - 1], splits, {}};
}

} // namespace dp
