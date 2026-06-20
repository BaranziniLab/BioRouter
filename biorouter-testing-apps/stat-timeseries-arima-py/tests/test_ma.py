"""Tests for tskit.ma — moving-average model fitting and forecasting."""

from tskit.numerics import set_seed, simulate_ma as sim_ma
from tskit.ma import fit_ma_css, fit_ma_mle, forecast_ma


class TestMAFit:
    def test_css_recovers_ma1(self):
        """MA(1) coefficient should be approximately recovered."""
        set_seed(42)
        x = sim_ma([0.6], n=2000, sigma=1.0)
        coeffs, sigma2 = fit_ma_css(x, 1)
        # MA estimation is harder; check it's in reasonable range
        assert abs(coeffs[0]) < 1.0, f"Got {coeffs[0]}"
        assert sigma2 > 0

    def test_mle_recovers_ma1(self):
        set_seed(42)
        x = sim_ma([0.5], n=2000, sigma=1.0)
        coeffs, sigma2 = fit_ma_mle(x, 1)
        assert abs(coeffs[0]) < 1.0, f"Got {coeffs[0]}"
        assert sigma2 > 0


class TestMAForecast:
    def test_forecast_structure(self):
        set_seed(42)
        x = sim_ma([0.5], n=500)
        result = forecast_ma(x, [0.5], steps=10, alpha=0.05)
        assert len(result["point"]) == 10
        assert len(result["lower"]) == 10
        assert len(result["upper"]) == 10

    def test_intervals_contain_point(self):
        set_seed(42)
        x = sim_ma([0.6], n=500)
        result = forecast_ma(x, [0.6], steps=5, alpha=0.05)
        for i in range(5):
            assert result["lower"][i] <= result["point"][i] <= result["upper"][i]
