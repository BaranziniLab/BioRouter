"""Tests for tskit.arima — ARIMA model with differencing."""

import math
from tskit.numerics import set_seed, simulate_ar, mean, variance, diff, undiff
from tskit.arima import fit_arima, predict_arima, forecast_arima, simulate_arima


class TestARIMASimulation:
    def test_simulate_arima_d1(self):
        """Simulated ARIMA(1,1,0) should have unit-root behavior."""
        set_seed(42)
        x = simulate_arima([0.5], [], d=1, n=500)
        assert len(x) == 500
        # Differenced should be stationary-ish
        d1 = diff(x, 1)
        assert abs(mean(d1)) < 1.0

    def test_simulate_arima_d0(self):
        """ARIMA(p,0,q) is just ARMA."""
        set_seed(42)
        x = simulate_arima([0.6], [0.3], d=0, n=500)
        assert len(x) == 500


class TestARIMAFit:
    def test_fit_ar1_d1(self):
        """Fit ARIMA(1,1,0) and check structure."""
        set_seed(42)
        x = simulate_arima([0.5], [], d=1, n=500)
        model = fit_arima(x, p=1, d=1, q=0)
        assert model["d"] == 1
        assert model["p"] == 1
        assert model["q"] == 0
        assert len(model["ar_coeffs"]) == 1
        assert model["sigma2"] > 0

    def test_fit_ma1_d1(self):
        """Fit ARIMA(0,1,1)."""
        set_seed(42)
        x = simulate_arima([], [0.5], d=1, n=500)
        model = fit_arima(x, p=0, d=1, q=1)
        assert model["d"] == 1
        assert len(model["ma_coeffs"]) == 1


class TestARIMAForecast:
    def test_forecast_structure(self):
        set_seed(42)
        x = simulate_arima([0.5], [], d=1, n=500)
        model = fit_arima(x, p=1, d=1, q=0)
        fc = forecast_arima(model, steps=10)
        assert len(fc["point"]) == 10
        assert len(fc["lower"]) == 10
        assert len(fc["upper"]) == 10
        # Lower < point < upper
        for i in range(10):
            assert fc["lower"][i] <= fc["point"][i] <= fc["upper"][i]

    def test_forecast_continuity(self):
        """Forecast should be roughly continuous with the series end."""
        set_seed(42)
        # Use a simple trend series
        x = [i * 0.1 for i in range(200)]
        model = fit_arima(x, p=1, d=1, q=0)
        fc = predict_arima(model, steps=1)
        # With d=1, 1-step forecast should be roughly x[-1] + estimated trend
        # The last difference is ~0.1, so forecast ≈ x[-1] + 0.1 ≈ 19.9 + 0.1 = 20.0
        assert abs(fc[0] - x[-1] - 0.1) < 2.0
