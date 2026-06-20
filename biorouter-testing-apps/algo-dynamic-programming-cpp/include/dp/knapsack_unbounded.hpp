#pragma once
#include "dp/common.hpp"

namespace dp {

/// Unbounded Knapsack: maximize value, each item may be taken multiple times.
/// @param weights item weights (positive)
/// @param values  item values  (positive)
/// @param capacity knapsack capacity
/// @return DpResult where value=max profit, solution=indices of chosen items (may repeat)
DpResult knapsack_unbounded(const std::vector<int>& weights,
                            const std::vector<int>& values,
                            int capacity);

} // namespace dp
