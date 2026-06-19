#include "dp/lcs.hpp"
#include <algorithm>

namespace dp {

// Generic LCS over vectors of int
DpResult lcs(const std::vector<int>& a, const std::vector<int>& b) {
    int m = static_cast<int>(a.size());
    int n = static_cast<int>(b.size());
    if (m == 0 || n == 0)
        return {0, {}, {}};

    // dp[i][j] = LCS length of a[0..i-1], b[0..j-1]
    std::vector<std::vector<int>> dp(static_cast<size_t>(m) + 1,
        std::vector<int>(static_cast<size_t>(n) + 1, 0));

    for (int i = 1; i <= m; ++i)
        for (int j = 1; j <= n; ++j)
            if (a[i - 1] == b[j - 1])
                dp[i][j] = dp[i - 1][j - 1] + 1;
            else
                dp[i][j] = std::max(dp[i - 1][j], dp[i][j - 1]);

    // Reconstruction: indices in A
    std::vector<int> indices;
    {
        int i = m, j = n;
        while (i > 0 && j > 0) {
            if (a[i - 1] == b[j - 1]) {
                indices.push_back(i - 1);
                --i; --j;
            } else if (dp[i - 1][j] >= dp[i][j - 1]) {
                --i;
            } else {
                --j;
            }
        }
        std::reverse(indices.begin(), indices.end());
    }
    return {dp[m][n], indices, {}};
}

// String overload: convert to int vectors
DpResult lcs(const std::string& a, const std::string& b) {
    std::vector<int> va(a.begin(), a.end());
    std::vector<int> vb(b.begin(), b.end());
    return lcs(va, vb);
}

} // namespace dp
