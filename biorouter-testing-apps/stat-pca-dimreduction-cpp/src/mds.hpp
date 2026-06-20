#pragma once
/// @file mds.hpp
/// Classical Multidimensional Scaling (MDS).
///
/// Given a distance matrix D (n×n), finds a low-dimensional embedding
/// that preserves pairwise distances.
///
/// Algorithm:
///   1. Double-center the squared distance matrix:
///      B = -½ J D² J,  where J = I - (1/n) 11ᵀ
///   2. Eigendecompose B = V Λ Vᵀ
///   3. Embedding: X = V_k Λ_k^{1/2} (top-k eigenvectors × √eigenvalues)
///
/// Reference: Torgerson (1952), Kruskal (1964).

#include "matrix.hpp"
#include "eigen.hpp"
#include <algorithm>
#include <cmath>

namespace mds {

struct MDSResult {
    lin::Matrix embedding;   // n × d embedding coordinates
    lin::Vector eigenvalues; // eigenvalues of centered matrix (descending)
    double stress;           // Kruskal stress-1
};

/// Classical MDS: given a distance matrix D (n×n), embed into d dimensions.
/// @param D     pairwise distance matrix (symmetric, zero diagonal)
/// @param d     target dimensionality (0 = auto, choose largest gap)
inline MDSResult classicalMDS(const lin::Matrix &D, int d = 0) {
    std::size_t n = D.rows();
    assert(D.cols() == n);

    // Step 1: Square distances elementwise
    lin::Matrix D2(n, n);
    for (std::size_t i = 0; i < n; ++i)
        for (std::size_t j = 0; j < n; ++j)
            D2(i, j) = D(i, j) * D(i, j);

    // Step 2: Double center: B = -0.5 * (D2 - rowMeans - colMeans + grandMean)
    // Equivalent to B = -0.5 * J D2 J where J = I - (1/n) 11ᵀ
    // Compute row means and grand mean of D2
    lin::Vector rowMeans(n, 0.0);
    double grandMean = 0.0;
    for (std::size_t i = 0; i < n; ++i) {
        for (std::size_t j = 0; j < n; ++j) rowMeans[i] += D2(i, j);
        rowMeans[i] /= static_cast<double>(n);
        grandMean += rowMeans[i];
    }
    grandMean /= static_cast<double>(n);

    lin::Matrix B(n, n);
    for (std::size_t i = 0; i < n; ++i)
        for (std::size_t j = 0; j < n; ++j)
            B(i, j) = -0.5 * (D2(i, j) - rowMeans[i] - rowMeans[j] + grandMean);

    // Step 3: Eigendecomposition (B is symmetric)
    auto eig = lin::jacobiEigen(B);

    // Eigenvalues sorted descending; pick top d
    if (d <= 0) {
        // Auto-select: use eigenvalues > 0
        d = 0;
        for (auto v : eig.eigenvalues)
            if (v > 1e-10) ++d;
        if (d == 0) d = 1;
    }
    std::size_t dk = static_cast<std::size_t>(d);

    // Step 4: Embedding X = V_k * diag(sqrt(max(0, λ_k)))
    lin::Matrix emb(n, dk);
    for (std::size_t j = 0; j < dk; ++j) {
        double s = std::sqrt(std::max(0.0, eig.eigenvalues[j]));
        for (std::size_t i = 0; i < n; ++i)
            emb(i, j) = eig.eigenvectors(i, j) * s;
    }

    // Step 5: Compute Kruskal stress-1
    // stress = sqrt( Σ (d_ij - δ̂_ij)² / Σ d̂_ij² )
    // where d_ij are original distances, δ̂_ij are embedded distances
    double numSum = 0.0, denSum = 0.0;
    for (std::size_t i = 0; i < n; ++i) {
        for (std::size_t j = i + 1; j < n; ++j) {
            double origDist = D(i, j);
            double embDist = 0.0;
            for (std::size_t k = 0; k < dk; ++k) {
                double diff = emb(i, k) - emb(j, k);
                embDist += diff * diff;
            }
            embDist = std::sqrt(embDist);
            double diff = origDist - embDist;
            numSum += diff * diff;
            denSum += origDist * origDist;
        }
    }
    double stress = (denSum > 1e-30) ? std::sqrt(numSum / denSum) : 0.0;

    MDSResult res;
    res.embedding = std::move(emb);
    res.eigenvalues.resize(dk);
    for (std::size_t j = 0; j < dk; ++j) res.eigenvalues[j] = eig.eigenvalues[j];
    res.stress = stress;
    return res;
}

/// Convenience: compute pairwise Euclidean distances from data matrix,
/// then run classical MDS.
inline MDSResult mdsFromData(const lin::Matrix &X, int d = 0) {
    lin::Matrix D = lin::pairwiseDistances(X);
    return classicalMDS(D, d);
}

}  // namespace mds
