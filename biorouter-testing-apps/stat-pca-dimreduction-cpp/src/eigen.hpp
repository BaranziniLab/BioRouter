#pragma once
/// @file eigen.hpp
/// Symmetric Jacobi eigenvalue algorithm for real symmetric matrices.
///
/// The classical Jacobi rotation method iteratively zeroes off-diagonal
/// elements of a symmetric matrix A by applying Givens rotations.
/// At convergence, D = Vᵀ A V is diagonal with eigenvalues on the diagonal,
/// and V contains the orthonormal eigenvectors as columns.
///
/// Reference: Golub & Van Loan, "Matrix Computations", §8.4.

#include "matrix.hpp"
#include <cmath>
#include <algorithm>

namespace lin {

/// Result of eigendecomposition
struct EigenResult {
    Vector eigenvalues;   // sorted descending
    Matrix eigenvectors;  // columns are eigenvectors, sorted by descending eigenvalue
};

/// Find the largest off-diagonal element |A(p,q)|
inline std::pair<std::size_t, std::size_t> maxOffDiag(const Matrix& A) {
    std::size_t p = 0, q = 1;
    double maxVal = 0.0;
    for (std::size_t i = 0; i < A.rows(); ++i)
        for (std::size_t j = i + 1; j < A.cols(); ++j) {
            double v = std::fabs(A(i, j));
            if (v > maxVal) { maxVal = v; p = i; q = j; }
        }
    return {p, q};
}

/// Sum of squares of off-diagonal elements
inline double offDiagSumSq(const Matrix& A) {
    double s = 0;
    for (std::size_t i = 0; i < A.rows(); ++i)
        for (std::size_t j = 0; j < A.cols(); ++j)
            if (i != j) s += A(i, j) * A(i, j);
    return s;
}

/// Jacobi eigenvalue algorithm for a symmetric matrix.
/// @param maxIter   maximum number of sweeps
/// @param tol       convergence tolerance on off-diagonal sum of squares
/// @return eigenvalues (ascending) and eigenvectors (columns)
inline EigenResult jacobiEigen(const Matrix& A, int maxIter = 200,
                               double tol = 1e-14) {
    assert(A.rows() == A.cols());
    std::size_t n = A.rows();
    Matrix V(n, n, 0.0);
    V.setIdentity();

    // Work on a copy
    Matrix D = A;

    double offNorm0 = offDiagSumSq(D);

    for (int iter = 0; iter < maxIter; ++iter) {
        auto [p, q] = maxOffDiag(D);
        double apq = D(p, q);
        if (std::fabs(apq) < 1e-15) break;

        // Compute rotation angle
        double app = D(p, p);
        double aqq = D(q, q);
        double tau = (aqq - app) / (2.0 * apq);
        double t;
        if (tau >= 0)
            t = 1.0 / (tau + std::sqrt(1.0 + tau * tau));
        else
            t = -1.0 / (-tau + std::sqrt(1.0 + tau * tau));
        double c = 1.0 / std::sqrt(1.0 + t * t);
        double s = t * c;

        // Apply Givens rotation G(p,q,theta) to D:  D' = Gᵀ D G
        // Update only rows/cols p and q
        Matrix Dnew = D;
        Dnew(p, p) = c * c * app - 2.0 * s * c * apq + s * s * aqq;
        Dnew(q, q) = s * s * app + 2.0 * s * c * apq + c * c * aqq;
        Dnew(p, q) = 0.0;
        Dnew(q, p) = 0.0;

        for (std::size_t r = 0; r < n; ++r) {
            if (r == p || r == q) continue;
            double drp = D(r, p);
            double drq = D(r, q);
            Dnew(r, p) = c * drp - s * drq;
            Dnew(p, r) = Dnew(r, p);
            Dnew(r, q) = s * drp + c * drq;
            Dnew(q, r) = Dnew(r, q);
        }
        D = Dnew;

        // Accumulate eigenvectors
        for (std::size_t r = 0; r < n; ++r) {
            double vp = V(r, p);
            double vq = V(r, q);
            V(r, p) = c * vp - s * vq;
            V(r, q) = s * vp + c * vq;
        }

        // Convergence check
        if (iter % n == 0) {
            double offNorm = offDiagSumSq(D);
            if (offNorm < tol * tol * offNorm0) break;
        }
    }

    // Extract eigenvalues and sort descending
    EigenResult res;
    res.eigenvalues.resize(n);
    for (std::size_t i = 0; i < n; ++i) res.eigenvalues[i] = D(i, i);

    // Create index array sorted by descending eigenvalue
    std::vector<std::size_t> idx(n);
    std::iota(idx.begin(), idx.end(), 0);
    std::sort(idx.begin(), idx.end(),
              [&](std::size_t a, std::size_t b) {
                  return res.eigenvalues[a] > res.eigenvalues[b];
              });

    // Reorder
    Vector evals(n);
    Matrix evecs(n, n);
    for (std::size_t k = 0; k < n; ++k) {
        evals[k] = res.eigenvalues[idx[k]];
        for (std::size_t r = 0; r < n; ++r)
            evecs(r, k) = V(r, idx[k]);
    }
    res.eigenvalues = std::move(evals);
    res.eigenvectors = std::move(evecs);

    return res;
}

}  // namespace lin
