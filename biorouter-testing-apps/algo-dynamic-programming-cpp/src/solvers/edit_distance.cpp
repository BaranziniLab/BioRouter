#include "dp/edit_distance.hpp"
#include <algorithm>

namespace dp {

namespace {

template <typename T>
DpResult edit_distance_impl(const std::vector<T>& a, const std::vector<T>& b) {
    int m = static_cast<int>(a.size());
    int n = static_cast<int>(b.size());

    // dp[i][j] = edit distance a[0..i-1] -> b[0..j-1]
    std::vector<std::vector<int>> dp(static_cast<size_t>(m) + 1,
        std::vector<int>(static_cast<size_t>(n) + 1, 0));

    for (int i = 0; i <= m; ++i) dp[i][0] = i;
    for (int j = 0; j <= n; ++j) dp[0][j] = j;

    for (int i = 1; i <= m; ++i)
        for (int j = 1; j <= n; ++j) {
            if (a[i - 1] == b[j - 1])
                dp[i][j] = dp[i - 1][j - 1];
            else
                dp[i][j] = 1 + std::min({dp[i - 1][j],      // delete
                                          dp[i][j - 1],      // insert
                                          dp[i - 1][j - 1]}); // replace
        }

    // Reconstruction: ops — 0=match, 1=replace, 2=insert, 3=delete
    std::vector<int> ops;
    {
        int i = m, j = n;
        while (i > 0 || j > 0) {
            if (i > 0 && j > 0 && a[i - 1] == b[j - 1]) {
                --i; --j; // match — no op emitted
            } else if (i > 0 && j > 0 && dp[i][j] == dp[i - 1][j - 1] + 1) {
                ops.push_back(1); --i; --j;  // replace
            } else if (j > 0 && dp[i][j] == dp[i][j - 1] + 1) {
                ops.push_back(2); --j;       // insert
            } else {
                ops.push_back(3); --i;       // delete
            }
        }
        std::reverse(ops.begin(), ops.end());
    }
    return {dp[m][n], ops, {}};
}

} // anonymous namespace

DpResult edit_distance(const std::string& a, const std::string& b) {
    std::vector<char> va(a.begin(), a.end());
    std::vector<char> vb(b.begin(), b.end());
    return edit_distance_impl(va, vb);
}

DpResult edit_distance(const std::vector<int>& a, const std::vector<int>& b) {
    return edit_distance_impl(a, b);
}

} // namespace dp
