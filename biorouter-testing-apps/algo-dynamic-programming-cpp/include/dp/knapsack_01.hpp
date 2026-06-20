#pragma once
#include "dp/common.hpp"

namespace dp {

/// 0/1 Knapsack: maximize value subject to weight capacity.
/// @param weights item weights (positive)
/// @param values  item values  (positive)
/// @param capacity knapsack capacity
/// @return DpResult where value=max profit, solution=indices of chosen items (0-based)
DpResult knapsack_01(const std::vector<int>& weights,
                     const std::vector<int>& values,
                     int capacity);

} // namespace dp
