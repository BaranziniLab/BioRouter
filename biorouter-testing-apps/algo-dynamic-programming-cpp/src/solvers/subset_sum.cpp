#include "dp/subset_sum.hpp"
#include <algorithm>
#include <numeric>

namespace dp {

DpResult subset_sum(const std::vector<int>& nums, int target) {
    int n = static_cast<int>(nums.size());
    if (target == 0) return {1, {}, {}};
    if (n == 0) return {0, {}, {}};

    // Check if target is achievable (bounded by sum of positives)
    long long total = 0;
    for (int x : nums) total += x;
    if (target < 0 || target > total) return {0, {}, {}};

    // dp[j] = 1 if sum j is achievable
    std::vector<uint8_t> dp(static_cast<size_t>(target) + 1, 0);
    dp[0] = 1;

    // For reconstruction: which item was last added to reach each sum
    std::vector<int> last_added(static_cast<size_t>(target) + 1, -1);

    for (int i = 0; i < n; ++i) {
        // iterate backwards to avoid reusing the same item
        for (int j = target; j >= nums[i]; --j) {
            if (!dp[j] && dp[j - nums[i]]) {
                dp[j] = 1;
                last_added[j] = i;
            }
        }
    }

    if (!dp[target]) return {0, {}, {}};

    // Reconstruction
    std::vector<int> chosen;
    int s = target;
    while (s > 0 && last_added[s] != -1) {
        int idx = last_added[s];
        chosen.push_back(nums[idx]);
        s -= nums[idx];
    }
    std::reverse(chosen.begin(), chosen.end());
    return {1, chosen, {}};
}

DpResult equal_partition(const std::vector<int>& nums) {
    long long total = 0;
    for (int x : nums) total += x;
    if (total % 2 != 0) return {0, {}, {}};
    return subset_sum(nums, static_cast<int>(total / 2));
}

} // namespace dp
