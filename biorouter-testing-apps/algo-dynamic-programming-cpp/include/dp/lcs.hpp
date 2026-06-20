#pragma once
#include "dp/common.hpp"
#include <string>

namespace dp {

/// Longest Common Subsequence of two sequences.
/// @return DpResult where value=LCS length, solution=LCS indices in seq A (0-based)
DpResult lcs(const std::string& a, const std::string& b);

/// Generic version over int vectors.
DpResult lcs(const std::vector<int>& a, const std::vector<int>& b);

} // namespace dp
