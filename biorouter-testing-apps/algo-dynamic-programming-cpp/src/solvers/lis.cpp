#include "dp/lis.hpp"
#include <algorithm>
#include <functional>

namespace dp {

DpResult lis(const std::vector<int>& seq) {
    int n = static_cast<int>(seq.size());
    if (n == 0) return {0, {}, {}};
    if (n == 1) return {1, {0}, {}};

    // tails[i] = smallest tail element of all increasing subsequences of length i+1
    // tail_idx[i] = index in seq of tails[i]
    // prev[k] = predecessor index of seq[k] in the LIS ending at k
    std::vector<int> tails, tail_idx;
    std::vector<int> prev(n, -1), dp_len(n, 0);

    for (int k = 0; k < n; ++k) {
        // binary search for position in tails
        auto it = std::lower_bound(tails.begin(), tails.end(), seq[k]);
        int pos = static_cast<int>(it - tails.begin());

        if (pos == static_cast<int>(tails.size())) {
            tails.push_back(seq[k]);
            tail_idx.push_back(k);
        } else {
            tails[pos] = seq[k];
            tail_idx[pos] = k;
        }

        dp_len[k] = pos;
        if (pos > 0)
            prev[k] = tail_idx[pos - 1];
    }

    int length = static_cast<int>(tails.size());

    // Reconstruction: backtrack from the element that ended the LIS
    std::vector<int> indices(length);
    int k = tail_idx[length - 1];
    for (int i = length - 1; i >= 0; --i) {
        indices[i] = k;
        k = prev[k];
    }

    return {length, indices, {}};
}

} // namespace dp
