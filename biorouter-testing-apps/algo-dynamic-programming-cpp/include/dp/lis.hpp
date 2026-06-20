#pragma once
#include "dp/common.hpp"

namespace dp {

/// Longest Increasing Subsequence — O(n log n) patience-sorting variant.
/// @return DpResult where value=LIS length, solution=indices of one LIS (0-based, sorted)
DpResult lis(const std::vector<int>& seq);

} // namespace dp
