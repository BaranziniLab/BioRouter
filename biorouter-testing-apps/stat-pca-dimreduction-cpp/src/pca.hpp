#pragma once
/// @file pca.hpp
/// Principal Component Analysis via eigen-decomposition and SVD.
///
/// Math:
///   Given data matrix X (n×p, n observations, p features):
///   1. Mean-center columns: X_c = X - 1·μᵀ
///   2. Covariance: C = (1/(n-1)) X_cᵀ X_c
///   3. Eigendecompose C = V Λ Vᵀ → principal components (eigenvectors of C)
///   4. Explained variance ratio: λ_k / Σ λ_i
///   5. Scores (projected data): T = X_c V
///   6. Loadings: L = V (eigenvectors scaled by √λ for correlation)
///   7. Reconstruct: X̂ = T Vᵀ + μ (from k components)
///
/// SVD path: X_c = U Σ Vᵀ, then C = V (Σ²/(n-1)) Vᵀ — same result.

#include "matrix.hpp"
#include "eigen.hpp"
#include "svd.hpp"
#include <algorithm>

namespace pca {

enum class Method { EIGEN, SVD };

struct PCAResult {
    lin::Matrix components;        // p × n_components (rows = principal axes)
    lin::Vector explainedVar;      // variance explained by each component
    lin::Vector explainedVarRatio; // cumulative or per-component ratio
    lin::Vector eigenvalues;       // all eigenvalues of covariance
    lin::Matrix scores;            // n × n_components (projected data)
    lin::Matrix loadings;          // p × n_components
    lin::Vector means;             // column means before centering
    int nComponents;               // number of components retained
};

/// Perform PCA.
/// @param X          input data (n×p), rows=observations, cols=features
/// @param nComp      number of components to retain (0 = min(n,p))
/// @param method     EIGEN (covariance eigendecomp) or SVD
inline PCAResult pca(const lin::Matrix &X, int nComp = 0,
                     Method method = Method::EIGEN) {
    std::size_t n = X.rows(), p = X.cols();
    std::size_t k = (nComp > 0) ? static_cast<std::size_t>(nComp)
                                : std::min(n, p);

    // Make a working copy and mean-center
    lin::Matrix Xc = X;
    lin::Vector means = Xc.meanCenterColumns();

    PCAResult res;
    res.means = means;
    res.nComponents = static_cast<int>(k);

    if (method == Method::EIGEN) {
        // Covariance matrix
        lin::Matrix C = Xc.covariance();  // p × p

        // Eigendecomposition
        auto eig = lin::jacobiEigen(C);

        // Eigenvalues already sorted descending
        res.eigenvalues = eig.eigenvalues;

        // Components (principal axes): first k eigenvectors (as rows)
        res.components = lin::Matrix(k, p);
        for (std::size_t j = 0; j < k; ++j)
            for (std::size_t i = 0; i < p; ++i)
                res.components(j, i) = eig.eigenvectors(i, j);

        // Scores: T = X_c * V (where V = first k columns of eigenvectors)
        // Xc is n×p, eigenvectors is p×p → take first k columns
        lin::Matrix Vk(p, k);
        for (std::size_t i = 0; i < p; ++i)
            for (std::size_t j = 0; j < k; ++j)
                Vk(i, j) = eig.eigenvectors(i, j);
        res.scores = Xc * Vk;  // n × k

        // Loadings = V_k * diag(√λ_k)
        res.loadings = lin::Matrix(p, k);
        for (std::size_t j = 0; j < k; ++j) {
            double s = std::sqrt(std::max(0.0, eig.eigenvalues[j]));
            for (std::size_t i = 0; i < p; ++i)
                res.loadings(i, j) = eig.eigenvectors(i, j) * s;
        }

    } else {
        // SVD path: X_c = U Σ Vᵀ
        auto sv = lin::svd(Xc);
        // V columns are right singular vectors = principal directions
        // Σ²/(n-1) = eigenvalues of covariance
        res.eigenvalues.resize(k);
        for (std::size_t j = 0; j < k; ++j) {
            double s = sv.sigma[j];
            res.eigenvalues[j] = s * s / static_cast<double>(n - 1);
        }

        // Components: first k rows of Vᵀ (= first k columns of V transposed)
        res.components = lin::Matrix(k, p);
        for (std::size_t j = 0; j < k; ++j)
            for (std::size_t i = 0; i < p; ++i)
                res.components(j, i) = sv.V(i, j);

        // Scores: T = X_c * V_k = U_k * Σ_k
        lin::Matrix Uk(n, k);
        for (std::size_t i = 0; i < n; ++i)
            for (std::size_t j = 0; j < k; ++j)
                Uk(i, j) = sv.U(i, j);
        lin::Matrix SigK = lin::Matrix::diagonal(
            lin::Vector(sv.sigma.begin(), sv.sigma.begin() + k));
        res.scores = Uk * SigK;

        // Loadings
        res.loadings = lin::Matrix(p, k);
        for (std::size_t j = 0; j < k; ++j) {
            double s = sv.sigma[j] / std::sqrt(static_cast<double>(n - 1));
            for (std::size_t i = 0; i < p; ++i)
                res.loadings(i, j) = sv.V(i, j) * s;
        }
    }

    // Explained variance ratio (per-component)
    double totalVar = 0;
    for (auto v : res.eigenvalues) totalVar += std::max(0.0, v);
    res.explainedVar.resize(k);
    res.explainedVarRatio.resize(k);
    for (std::size_t j = 0; j < k; ++j) {
        res.explainedVar[j] = std::max(0.0, res.eigenvalues[j]);
        res.explainedVarRatio[j] = (totalVar > 0) ? res.explainedVar[j] / totalVar : 0.0;
    }

    return res;
}

/// Transform new data using fitted PCA.
/// @param Xnew   new data (m×p)
/// @param res    PCA result from pca()
/// @return       projected data (m × nComponents)
lin::Matrix transform(const lin::Matrix &Xnew, const PCAResult &res) {
    assert(Xnew.cols() == res.means.size());
    std::size_t m = Xnew.rows();
    std::size_t k = static_cast<std::size_t>(res.nComponents);
    std::size_t p = Xnew.cols();

    // Mean-center using training means
    lin::Matrix Xc(m, p);
    for (std::size_t i = 0; i < m; ++i)
        for (std::size_t j = 0; j < p; ++j)
            Xc(i, j) = Xnew(i, j) - res.means[j];

    // Project: T = X_c * V_k
    lin::Matrix V(k, p);
    for (std::size_t j = 0; j < k; ++j)
        for (std::size_t i = 0; i < p; ++i)
            V(j, i) = res.components(j, i);

    return Xc * V.transpose();  // m × k (wait, V is k×p, need Xc * Vᵀ? No.)
    // Actually components is k×p where each row is a principal axis.
    // Projection = X_c * Vᵀ where V is k×p → (m×p)(p×k) = m×k.
    // So: Xc * components.transpose()? No.
    // components(j,i) = j-th principal axis, i-th feature → V[j][i]
    // Projection t_i = x_i · v_j for each j
    // So scores = Xc * Vᵀ where V = components (k×p) → Vᵀ is p×k
    // But Xc is m×p and Vᵀ is p×k, so Xc * Vᵀ = m×k. But we have components as k×p.
    // We need Xc * componentsᵀ to get m×k. componentsᵀ is p×k.
    // So: Xc * componentsᵀ doesn't work directly. Let me just do:
    //   scores(i,j) = sum_l Xc(i,l) * components(j,l)
    // = row i of Xc · row j of components
    lin::Matrix result(m, k);
    for (std::size_t i = 0; i < m; ++i)
        for (std::size_t j = 0; j < k; ++j) {
            double s = 0;
            for (std::size_t l = 0; l < p; ++l)
                s += Xc(i, l) * res.components(j, l);
            result(i, j) = s;
        }
    return result;
}

/// Reconstruct data from scores using first k components.
/// X̂ = scores * components + μ
lin::Matrix reconstruct(const lin::Matrix &scores, const PCAResult &res) {
    std::size_t m = scores.rows();
    std::size_t p = res.means.size();
    std::size_t k = static_cast<std::size_t>(res.nComponents);

    // X̂_centered = scores * components  (m×k)(k×p) = m×p
    lin::Matrix Xc(m, p, 0.0);
    for (std::size_t i = 0; i < m; ++i)
        for (std::size_t j = 0; j < p; ++j)
            for (std::size_t l = 0; l < k; ++l)
                Xc(i, j) += scores(i, l) * res.components(l, j);

    // Add back means
    for (std::size_t i = 0; i < m; ++i)
        for (std::size_t j = 0; j < p; ++j)
            Xc(i, j) += res.means[j];

    return Xc;
}

}  // namespace pca
