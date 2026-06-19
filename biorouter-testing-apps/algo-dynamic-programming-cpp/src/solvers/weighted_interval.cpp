#include "dp/weighted_interval.hpp"
#include <algorithm>

namespace dp {

DpResult weighted_interval(const std::vector<int>& starts,
                           const std::vector<int>& ends,
                           const std::vector<int>& weights) {
    int n = static_cast<int>(starts.size());
    if (n == 0) return {0, {}, {}};

    // Build sorted order by end time
    std::vector<int> idx(n);
    for (int i = 0; i < n; ++i) idx[i] = i;
    std::sort(idx.begin(), idx.end(), [&](int a, int b) {
        return ends[a] < ends[b];
    });

    // sorted arrays
    std::vector<int> s(n), e(n), w(n);
    for (int i = 0; i < n; ++i) {
        s[i] = starts[idx[i]];
        e[i] = ends[idx[i]];
        w[i] = weights[idx[i]];
    }

    // p[i] = largest index j < i such that interval j doesn't overlap i
    std::vector<int> p(n, -1);
    for (int i = 1; i < n; ++i) {
        // binary search for rightmost interval ending <= s[i]
        int lo = 0, hi = i - 1, best = -1;
        while (lo <= hi) {
            int mid = (lo + hi) / 2;
            if (e[mid] <= s[i]) { best = mid; lo = mid + 1; }
            else hi = mid - 1;
        }
        p[i] = best;
    }

    // dp[i] = best weight using intervals 0..i
    std::vector<long long> dp(static_cast<size_t>(n), 0);
    dp[0] = w[0];
    for (int i = 1; i < n; ++i) {
        long long include = w[i] + (p[i] >= 0 ? dp[p[i]] : 0);
        dp[i] = std::max(include, dp[i - 1]);
    }

    // Reconstruction
    std::vector<int> chosen;
    {
        int i = n - 1;
        while (i >= 0) {
            long long include = w[i] + (p[i] >= 0 ? dp[p[i]] : 0);
            if (i == 0 || include >= dp[i - 1]) {
                chosen.push_back(idx[i]); // original index
                i = p[i];
            } else {
                --i;
            }
        }
        std::reverse(chosen.begin(), chosen.end());
    }

    return {dp[n - 1], chosen, {}};
}

} // namespace dp
