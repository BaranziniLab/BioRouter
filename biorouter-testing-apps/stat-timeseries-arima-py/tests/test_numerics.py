"""Tests for tskit.numerics — core linear algebra, statistics, and simulation."""

import math
from tskit.numerics import (
    zeros, ones, dot, mat_vec_mul,
    solve_toeplitz, cholesky_solve, lstsq,
    mean, variance, std, cumsum, diff, undiff,
    acf, pacf, adf_test,
    simulate_ar, simulate_ma, simulate_arma,
    set_seed,
)


class TestLinearAlgebra:
    def test_dot(self):
        assert dot([1, 2, 3], [4, 5, 6]) == 32

    def test_mat_vec_mul(self):
        A = [[1, 2], [3, 4]]
        v = [5, 6]
        result = mat_vec_mul(A, v)
        assert result == [17, 39]

    def test_solve_toeplitz_trivial(self):
        # Toeplitz(1, 0.5) → 2x2 system
        r = [1.0, 0.5, 0.25]
        a = solve_toeplitz(r)
        # Check residual
        T = [[1.0, 0.5], [0.5, 1.0]]
        rhs = [0.5, 0.25]
        product = mat_vec_mul(T, a)
        for p, r_val in zip(product, rhs):
            assert abs(p - r_val) < 1e-6

    def test_cholesky_solve(self):
        A = [[4, 2], [2, 3]]
        b = [8, 7]
        x = cholesky_solve(A, b)
        # Verify A x ≈ b
        for i in range(2):
            assert abs(sum(A[i][j] * x[j] for j in range(2)) - b[i]) < 1e-8

    def test_lstsq(self):
        # Fit y = 3x (no intercept in design)
        A = [[1], [2], [3], [4], [5]]
        b = [3, 6, 9, 12, 15]  # y = 3*x
        x = lstsq(A, b)
        assert abs(x[0] - 3.0) < 1e-6

    def test_lstsq_with_intercept(self):
        # Fit y = 2x + 1 using [1, x] design
        A = [[1, 1], [1, 2], [1, 3], [1, 4], [1, 5]]
        b = [3, 5, 7, 9, 11]  # 2*x + 1
        x = lstsq(A, b)
        assert abs(x[0] - 1.0) < 1e-6  # intercept
        assert abs(x[1] - 2.0) < 1e-6  # slope


class TestStatistics:
    def test_mean(self):
        assert mean([1, 2, 3, 4, 5]) == 3.0

    def test_variance(self):
        v = variance([1, 2, 3, 4, 5], ddof=1)
        assert abs(v - 2.5) < 1e-10

    def test_cumsum(self):
        assert cumsum([1, 2, 3]) == [1, 3, 6]

    def test_diff(self):
        assert diff([1, 3, 6, 10]) == [2, 3, 4]

    def test_diff_order2(self):
        # diff([1,4,9,16]) = [3,5,7]; diff again = [2,2]
        assert diff([1, 4, 9, 16], d=2) == [2, 2]

    def test_undiff_roundtrip(self):
        original = [10, 13, 17, 22, 28]
        d = diff(original, 1)
        recovered = undiff([original[0]], d)
        # undiff includes the initial value
        assert len(recovered) == len(d) + 1
        for a, b in zip(original, recovered):
            assert abs(a - b) < 1e-10

    def test_undiff_order2_roundtrip(self):
        original = [1, 4, 9, 16, 25]
        d = diff(original, 2)
        # Seeds: x_0 and x_1 - x_0
        seeds = [original[0], original[1] - original[0]]
        recovered = undiff(seeds, d)
        assert len(recovered) == len(original)
        for a, b in zip(original, recovered):
            assert abs(a - b) < 1e-10

    def test_arima_seeds(self):
        from tskit.numerics import arima_seeds
        original = [1, 4, 9, 16, 25]
        seeds = arima_seeds(original, 2)
        assert seeds == [1, 3]  # x_0=1, x_1-x_0=3


class TestACF:
    def test_acf_lag0_is_one(self):
        r = acf([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], nlags=5)
        assert abs(r[0] - 1.0) < 1e-10

    def test_acf_white_noise(self):
        set_seed(42)
        x = [0.1 * (i % 3 - 1) for i in range(200)]
        r = acf(x, nlags=5)
        # Deterministic, not white noise, but lag 0 = 1
        assert abs(r[0] - 1.0) < 1e-10

    def test_pacf_white_noise(self):
        set_seed(42)
        x = [0.1 * (i % 3 - 1) for i in range(200)]
        p = pacf(x, nlags=10)
        assert abs(p[0] - 1.0) < 1e-10


class TestADF:
    def test_adf_stationary(self):
        set_seed(123)
        # White noise should be stationary
        x = [0.0] * 200
        for i in range(200):
            from tskit.numerics import randn
            x[i] = randn()
        result = adf_test(x)
        assert result["reject_5pct"] is True

    def test_adf_returns_dict(self):
        result = adf_test([1, 2, 3, 4, 5])
        assert "statistic" in result
        assert "p_value" in result


class TestSimulation:
    def test_simulate_ar(self):
        set_seed(99)
        x = simulate_ar([0.5], n=500, sigma=1.0)
        assert len(x) == 500
        assert abs(mean(x)) < 0.5  # Should be near zero

    def test_simulate_ma(self):
        set_seed(42)
        x = simulate_ma([0.6], n=500, sigma=1.0)
        assert len(x) == 500

    def test_simulate_arma(self):
        set_seed(77)
        x = simulate_arma([0.4], [0.3], n=500, sigma=1.0)
        assert len(x) == 500
