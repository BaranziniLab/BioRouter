#pragma once
#include "dp/common.hpp"
#include <string>

namespace dp {

/// Levenshtein edit distance (insert, delete, replace cost 1).
/// @return DpResult where value=edit distance, solution=sequence of ops (0=match,1=replace,2=insert,3=delete)
DpResult edit_distance(const std::string& a, const std::string& b);

/// Generic version over int vectors.
DpResult edit_distance(const std::vector<int>& a, const std::vector<int>& b);

} // namespace dp
