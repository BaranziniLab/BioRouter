#pragma once
#include "dp/common.hpp"

namespace dp {

/// Grid Minimum Path Sum: find path from top-left to bottom-right minimizing sum.
/// Only moves: right or down.
/// @param grid row-major 2D grid of non-negative costs
/// @return DpResult where value=min cost, solution=sequence of moves (0=right, 1=down)
DpResult grid_min_path(const std::vector<std::vector<int>>& grid);

} // namespace dp
