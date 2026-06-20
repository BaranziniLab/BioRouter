"""Tests for tskit.ar — autoregressive model fitting and forecasting."""

import math
from tskit.numerics import set_seed, simulate_ar as sim_ar, mean, variance
from tskit.ar import fit_yule_walker, fit_least_squares, predict_ar, forecast_ar


class TestYuleWalker:
    def test_recovers_ar1(self):
        """AR(1) with coefficient 0.5 should be recovered."""
        set_seed(42)
        x = sim_ar([0.5], n=2000, sigma=1.0)
        coeffs, sigma2 = fit_yule_walker(x, 1)
        assert abs(coeffs[0] - 0.5) < 0.15, f"Got {coeffs[0]}"

    def test_recovers_ar2(self):
        """AR(2) with known coefficients."""
        set_seed(123)
        # AR(2): x_t = 0.6*x_{t-1} - 0.2*x_{t-2}
        x = sim_ar([0.6, -0.2], n=2000, sigma=1.0)
        coeffs, sigma2 = fit_yule_walker(x, 2)
        assert abs(coeffs[0] - 0.6) < 0.15, f"Got {coeffs[0]}"
        assert abs(coeffs[1] - (-0.2)) < 0.15, f"Got {coeffs[1]}"


class TestLeastSquares:
    def test_recovers_ar1(self):
        set_seed(42)
        x = sim_ar([0.7], n=2000, sigma=1.0)
        coeffs, sigma2 = fit_least_squares(x, 1)
        assert abs(coeffs[0] - 0.7) < 0.15, f"Got {coeffs[0]}"

    def test_positive_sigma2(self):
        set_seed(99)
        x = sim_ar([0.3], n=500)
        _, sigma2 = fit_least_squares(x, 1)
        assert sigma2 > 0


class TestARForecast:
    def test_forecast_direction(self):
        """Strong positive AR(1) should forecast in correct direction from last value."""
        set_seed(42)
        x = sim_ar([0.8], n=500, sigma=1.0)
        coeffs, _ = fit_least_squares(x, 1)
        fc = predict_ar(x, coeffs, steps=10)
        # First forecast should be approximately coeffs[0] * x[-1]
        expected = coeffs[0] * x[-1]
        assert abs(fc[0] - expected) < 1e-10

    def test_forecast_intervals(self):
        set_seed(42)
        x = sim_ar([0.5], n=500, sigma=1.0)
        result = forecast_ar(x, [0.5], steps=10, alpha=0.05)
        assert "point" in result
        assert "lower" in result
        assert "upper" in result
        assert len(result["point"]) == 10
        # Intervals should widen with horizon
        for i in range(1, 10):
            w0 = result["upper"][0] - result["lower"][0]
            wi = result["upper"][i] - result["lower"][i]
            assert wi >= w0 * 0.5  # Allow some tolerance

    def test_point_forecast_matches(self):
        set_seed(42)
        x = sim_ar([0.6], n=200, sigma=0.5)
        coeffs, sigma2 = fit_least_squares(x, 1)
        point = predict_ar(x, coeffs, steps=1)
        # First forecast should be close to coeffs[0] * x[-1]
        expected = coeffs[0] * x[-1]
        assert abs(point[0] - expected) < 1e-10
