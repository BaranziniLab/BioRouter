#pragma once
#include "dp/common.hpp"

namespace dp {

/// Coin Change — minimum number of coins to make amount.
/// @param coins available coin denominations
/// @param amount target amount
/// @return DpResult where value=min coins (or -1 if impossible), solution=coin denominations used
DpResult coin_change_min(const std::vector<int>& coins, int amount);

/// Coin Change — number of distinct ways to make amount.
/// @return DpResult where value=count of ways, solution is empty (count only)
DpResult coin_change_count(const std::vector<int>& coins, int amount);

} // namespace dp
