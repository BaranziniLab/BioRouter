#pragma once
#include "dp/common.hpp"

namespace dp {

/// Matrix-Chain Multiplication: find parenthesization minimizing scalar multiplications.
/// @param dims dimensions vector of length n+1 for n matrices (matrix i is dims[i] x dims[i+1])
/// @return DpResult where value=min scalar multiplications, solution=split points for parenthesization
DpResult matrix_chain(const std::vector<int>& dims);

} // namespace dp
