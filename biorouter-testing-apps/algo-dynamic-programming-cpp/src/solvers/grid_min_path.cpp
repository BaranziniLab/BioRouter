#include "dp/grid_min_path.hpp"
#include <algorithm>

namespace dp {

DpResult grid_min_path(const std::vector<std::vector<int>>& grid) {
    int rows = static_cast<int>(grid.size());
    if (rows == 0) return {0, {}, {}};
    int cols = static_cast<int>(grid[0].size());
    if (cols == 0) return {0, {}, {}};

    // dp[i][j] = min cost from (0,0) to (i,j)
    std::vector<std::vector<long long>> dp(static_cast<size_t>(rows),
        std::vector<long long>(static_cast<size_t>(cols), 0));

    dp[0][0] = grid[0][0];
    for (int j = 1; j < cols; ++j) dp[0][j] = dp[0][j - 1] + grid[0][j];
    for (int i = 1; i < rows; ++i) dp[i][0] = dp[i - 1][0] + grid[i][0];

    for (int i = 1; i < rows; ++i)
        for (int j = 1; j < cols; ++j)
            dp[i][j] = std::min(dp[i - 1][j], dp[i][j - 1]) + grid[i][j];

    // Reconstruction: 0=right, 1=down (from (0,0) to (rows-1,cols-1))
    std::vector<int> moves;
    {
        int i = rows - 1, j = cols - 1;
        while (i > 0 || j > 0) {
            if (i == 0) { moves.push_back(0); --j; }
            else if (j == 0) { moves.push_back(1); --i; }
            else if (dp[i - 1][j] <= dp[i][j - 1]) { moves.push_back(1); --i; }
            else { moves.push_back(0); --j; }
        }
        std::reverse(moves.begin(), moves.end());
    }

    return {dp[rows - 1][cols - 1], moves, {}};
}

} // namespace dp
