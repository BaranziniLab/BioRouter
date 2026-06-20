#pragma once
#include "dp/common.hpp"

namespace dp {

/// Weighted Interval Scheduling: select non-overlapping intervals to maximize weight.
/// @param starts  interval start times
/// @param ends    interval end times (exclusive)
/// @param weights interval weights/values
/// @return DpResult where value=max weight, solution=indices of chosen intervals (0-based)
DpResult weighted_interval(const std::vector<int>& starts,
                           const std::vector<int>& ends,
                           const std::vector<int>& weights);

} // namespace dp
