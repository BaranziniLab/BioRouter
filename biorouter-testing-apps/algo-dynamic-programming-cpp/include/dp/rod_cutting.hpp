#pragma once
#include "dp/common.hpp"

namespace dp {

/// Rod Cutting: maximize revenue by cutting a rod of length n.
/// @param prices prices[i] = revenue for piece of length i+1 (size n)
/// @return DpResult where value=max revenue, solution=lengths of pieces cut
DpResult rod_cutting(const std::vector<int>& prices);

} // namespace dp
