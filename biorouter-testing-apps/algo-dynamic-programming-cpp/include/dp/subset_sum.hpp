#pragma once
#include "dp/common.hpp"

namespace dp {

/// Subset Sum: determine if a subset sums to target.
/// @return DpResult where value=1 if possible else 0, solution=chosen elements
DpResult subset_sum(const std::vector<int>& nums, int target);

/// Partition: can the set be partitioned into two subsets with equal sum?
/// @return DpResult where value=1 if possible else 0, solution=one partition
DpResult equal_partition(const std::vector<int>& nums);

} // namespace dp
