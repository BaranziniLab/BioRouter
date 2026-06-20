#pragma once
#include <vector>
#include <limits>
#include <stdexcept>

namespace dp {

/// Result of a DP computation: optimal value + reconstructed solution path.
struct DpResult {
    long long value;                   ///< optimal objective value
    std::vector<int> solution;         ///< reconstructed solution (meaning varies per problem)
    std::vector<std::vector<int>> solution_2d; ///< for 2-D reconstructions (e.g. matrix-chain splits)
};

constexpr long long INF = std::numeric_limits<long long>::max() / 2;

} // namespace dp
