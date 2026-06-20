#pragma once
/// @file matrix.hpp
/// Dense row-major Matrix and Vector types with basic linear algebra ops.
/// All arithmetic is done from scratch — no BLAS/LAPACK dependency.

#include <algorithm>
#include <cassert>
#include <cmath>
#include <cstddef>
#include <initializer_list>
#include <iostream>
#include <numeric>
#include <random>
#include <stdexcept>
#include <string>
#include <vector>

namespace lin {

// ────────────────────────────────────────────────────────
//  Vector (alias for std::vector<double> with helpers)
// ────────────────────────────────────────────────────────
using Vector = std::vector<double>;

/// Element-wise addition
inline Vector operator+(const Vector& a, const Vector& b) {
    assert(a.size() == b.size());
    Vector r(a.size());
    for (std::size_t i = 0; i < a.size(); ++i) r[i] = a[i] + b[i];
    return r;
}
/// Element-wise subtraction
inline Vector operator-(const Vector& a, const Vector& b) {
    assert(a.size() == b.size());
    Vector r(a.size());
    for (std::size_t i = 0; i < a.size(); ++i) r[i] = a[i] - b[i];
    return r;
}
/// Scalar multiply
inline Vector operator*(double s, const Vector& v) {
    Vector r(v.size());
    for (std::size_t i = 0; i < v.size(); ++i) r[i] = s * v[i];
    return r;
}
/// Dot product
inline double dot(const Vector& a, const Vector& b) {
    assert(a.size() == b.size());
    return std::inner_product(a.begin(), a.end(), b.begin(), 0.0);
}
/// L2 norm
inline double norm(const Vector& v) { return std::sqrt(dot(v, v)); }

// ────────────────────────────────────────────────────────
//  Matrix — row-major storage: element (i,j) = data_[i*cols_+j]
// ────────────────────────────────────────────────────────
class Matrix {
public:
    Matrix() : rows_(0), cols_(0) {}
    Matrix(std::size_t rows, std::size_t cols, double val = 0.0)
        : rows_(rows), cols_(cols), data_(rows * cols, val) {}
    Matrix(std::initializer_list<std::initializer_list<double>> il)
        : rows_(il.size()), cols_(il.empty() ? 0 : il.begin()->size()) {
        data_.reserve(rows_ * cols_);
        for (auto& row : il)
            for (double v : row) data_.push_back(v);
    }

    // ── accessors ──
    std::size_t rows() const { return rows_; }
    std::size_t cols() const { return cols_; }
    double* data() { return data_.data(); }
    const double* data() const { return data_.data(); }

    double& operator()(std::size_t i, std::size_t j) {
        assert(i < rows_ && j < cols_);
        return data_[i * cols_ + j];
    }
    double operator()(std::size_t i, std::size_t j) const {
        assert(i < rows_ && j < cols_);
        return data_[i * cols_ + j];
    }

    /// Return row i as a Vector
    Vector row(std::size_t i) const {
        return Vector(data_.begin() + i * cols_,
                      data_.begin() + (i + 1) * cols_);
    }
    /// Return column j as a Vector
    Vector col(std::size_t j) const {
        Vector r(rows_);
        for (std::size_t i = 0; i < rows_; ++i) r[i] = (*this)(i, j);
        return r;
    }

    // ── in-place mutations ──
    void fill(double v) { std::fill(data_.begin(), data_.end(), v); }
    void setZero() { fill(0.0); }
    void setIdentity() {
        setZero();
        for (std::size_t i = 0; i < std::min(rows_, cols_); ++i)
            (*this)(i, i) = 1.0;
    }

    // ── arithmetic ──
    Matrix operator+(const Matrix& b) const {
        assert(rows_ == b.rows_ && cols_ == b.cols_);
        Matrix r(rows_, cols_);
        for (std::size_t i = 0; i < data_.size(); ++i)
            r.data_[i] = data_[i] + b.data_[i];
        return r;
    }
    Matrix operator-(const Matrix& b) const {
        assert(rows_ == b.rows_ && cols_ == b.cols_);
        Matrix r(rows_, cols_);
        for (std::size_t i = 0; i < data_.size(); ++i)
            r.data_[i] = data_[i] - b.data_[i];
        return r;
    }
    Matrix operator*(double s) const {
        Matrix r(rows_, cols_);
        for (std::size_t i = 0; i < data_.size(); ++i) r.data_[i] = data_[i] * s;
        return r;
    }

    /// Matrix multiply  C = A * B   (naive O(n³), fine for small-medium)
    Matrix operator*(const Matrix& b) const {
        assert(cols_ == b.rows_);
        Matrix c(rows_, b.cols_, 0.0);
        for (std::size_t i = 0; i < rows_; ++i)
            for (std::size_t k = 0; k < cols_; ++k) {
                double aik = (*this)(i, k);
                for (std::size_t j = 0; j < b.cols_; ++j)
                    c(i, j) += aik * b(k, j);
            }
        return c;
    }

    /// Transpose
    Matrix transpose() const {
        Matrix t(cols_, rows_);
        for (std::size_t i = 0; i < rows_; ++i)
            for (std::size_t j = 0; j < cols_; ++j)
                t(j, i) = (*this)(i, j);
        return t;
    }

    // ── reductions ──
    /// Column means (1 × cols)
    Vector colMeans() const {
        Vector m(cols_, 0.0);
        for (std::size_t j = 0; j < cols_; ++j) {
            for (std::size_t i = 0; i < rows_; ++i) m[j] += (*this)(i, j);
            m[j] /= static_cast<double>(rows_);
        }
        return m;
    }

    /// Row means (rows × 1)
    Vector rowMeans() const {
        Vector m(rows_, 0.0);
        for (std::size_t i = 0; i < rows_; ++i) {
            for (std::size_t j = 0; j < cols_; ++j) m[i] += (*this)(i, j);
            m[i] /= static_cast<double>(cols_);
        }
        return m;
    }

    /// Mean-center each column in-place; return column means before centering
    Vector meanCenterColumns() {
        Vector means = colMeans();
        for (std::size_t j = 0; j < cols_; ++j)
            for (std::size_t i = 0; i < rows_; ++i)
                (*this)(i, j) -= means[j];
        return means;
    }

    /// Covariance matrix  Σ = (1/(n-1)) Xᵀ X  (columns are variables)
    /// Assumes X is already mean-centered.
    Matrix covariance() const {
        Matrix xt = transpose();
        Matrix cov = xt * (*this);
        double scale = 1.0 / static_cast<double>(rows_ - 1);
        return cov * scale;
    }

    /// Gram matrix  G = X Xᵀ  (rows are observations)
    Matrix gram() const { return (*this) * transpose(); }

    /// Frobenius norm
    double frobeniusNorm() const {
        double s = 0;
        for (double v : data_) s += v * v;
        return std::sqrt(s);
    }

    // ── factory helpers ──
    static Matrix identity(std::size_t n) {
        Matrix m(n, n);
        m.setIdentity();
        return m;
    }

    /// Random matrix with entries drawn from N(0,1)
    static Matrix random(std::size_t rows, std::size_t cols,
                         unsigned seed = 42) {
        std::mt19937 gen(seed);
        std::normal_distribution<double> dist(0.0, 1.0);
        Matrix m(rows, cols);
        for (std::size_t i = 0; i < rows * cols; ++i)
            m.data_[i] = dist(gen);
        return m;
    }

    /// Diagonal matrix from a vector
    static Matrix diagonal(const Vector& v) {
        Matrix m(v.size(), v.size());
        for (std::size_t i = 0; i < v.size(); ++i) m(i, i) = v[i];
        return m;
    }

    /// Print to stream
    void print(std::ostream& os = std::cout, int precision = 6) const {
        os.setf(std::ios::fixed);
        os.precision(precision);
        for (std::size_t i = 0; i < rows_; ++i) {
            for (std::size_t j = 0; j < cols_; ++j)
                os << (*this)(i, j) << " ";
            os << "\n";
        }
    }

private:
    std::size_t rows_, cols_;
    std::vector<double> data_;
};

// ── free functions ──
/// Matrix-vector multiply: y = A * x
inline Vector operator*(const Matrix& A, const Vector& x) {
    assert(A.cols() == x.size());
    Vector y(A.rows(), 0.0);
    for (std::size_t i = 0; i < A.rows(); ++i)
        for (std::size_t j = 0; j < A.cols(); ++j)
            y[i] += A(i, j) * x[j];
    return y;
}

/// Euclidean distance between two row vectors stored in a Matrix
inline double rowDistance(const Matrix& m, std::size_t i, std::size_t j) {
    double s = 0;
    for (std::size_t k = 0; k < m.cols(); ++k) {
        double d = m(i, k) - m(j, k);
        s += d * d;
    }
    return std::sqrt(s);
}

/// Build full pairwise distance matrix (rows of m)
inline Matrix pairwiseDistances(const Matrix& m) {
    Matrix D(m.rows(), m.rows());
    for (std::size_t i = 0; i < m.rows(); ++i)
        for (std::size_t j = 0; j < m.rows(); ++j)
            D(i, j) = rowDistance(m, i, j);
    return D;
}

}  // namespace lin
