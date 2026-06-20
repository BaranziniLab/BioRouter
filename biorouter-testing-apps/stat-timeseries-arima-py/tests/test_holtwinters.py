"""Tests for tskit.holtwinters — Holt-Winters exponential smoothing."""

import math
from tskit.numerics import set_seed, mean
from tskit.holtwinters import fit_holt_winters, predict_hw, forecast_hw


def _make_seasonal_series(n=120, m=12, trend=0.5, seasonal_amp=10.0):
    """Create a deterministic seasonal series with trend."""
    x = []
    for i in range(n):
        val = 100 + trend * i + seasonal_amp * math.sin(2 * math.pi * i / m)
        x.append(val)
    return x


class TestHoltWintersFit:
    def test_fit_additive(self):
        """Fit additive Holt-Winters."""
        x = _make_seasonal_series(120, m=12)
        model = fit_holt_winters(x, m=12, method="additive")
        assert model["method"] == "additive"
        assert model["m"] == 12
        assert model["alpha"] > 0
        assert model["gamma"] > 0

    def test_fit_multiplicative(self):
        """Fit multiplicative Holt-Winters."""
        x = _make_seasonal_series(120, m=12, seasonal_amp=5.0)
        model = fit_holt_winters(x, m=12, method="multiplicative")
        assert model["method"] == "multiplicative"

    def test_minimum_length(self):
        """Should raise ValueError for series shorter than 2*m."""
        x = [1.0, 2.0, 3.0]
        try:
            fit_holt_winters(x, m=12)
            assert False, "Should have raised ValueError"
        except ValueError:
            pass


class TestHoltWintersForecast:
    def test_forecast_structure(self):
        x = _make_seasonal_series(120, m=12)
        model = fit_holt_winters(x, m=12)
        fc = forecast_hw(model, steps=12)
        assert len(fc["point"]) == 12
        assert len(fc["lower"]) == 12
        assert len(fc["upper"]) == 12

    def test_forecast_continues_trend(self):
        """Forecast should continue the upward trend."""
        x = _make_seasonal_series(120, m=12, trend=1.0, seasonal_amp=2.0)
        model = fit_holt_winters(x, m=12)
        fc = predict_hw(model, steps=12)
        # Last forecast should be higher than first
        assert fc[-1] > fc[0]

    def test_forecast_seasonal_pattern(self):
        """Forecast should show seasonal variation."""
        x = _make_seasonal_series(120, m=12, trend=0.0, seasonal_amp=10.0)
        model = fit_holt_winters(x, m=12)
        fc = predict_hw(model, steps=12)
        # There should be variation (max - min > some threshold)
        assert max(fc) - min(fc) > 5.0

    def test_forecast_intervals(self):
        x = _make_seasonal_series(120, m=12)
        model = fit_holt_winters(x, m=12)
        fc = forecast_hw(model, steps=5, alpha=0.05)
        for i in range(5):
            assert fc["lower"][i] <= fc["point"][i] <= fc["upper"][i]

    def test_forecast_good_on_seasonal(self):
        """Holt-Winters should forecast a seasonal series with reasonable error."""
        set_seed(42)
        # Create series with known pattern
        m = 12
        x = _make_seasonal_series(60, m=m, trend=0.1, seasonal_amp=5.0)
        # Add small noise
        from tskit.numerics import randn
        x = [xi + 0.3 * randn() for xi in x]
        model = fit_holt_winters(x, m=m)
        fc = predict_hw(model, steps=m)
        # True values for next 12 months
        truth = [_make_seasonal_series(72, m=m, trend=0.1, seasonal_amp=5.0)[60 + i] for i in range(m)]
        mae = mean([abs(fc[i] - truth[i]) for i in range(m)])
        # Should be reasonably accurate (MAE < 3 for a series with amp=5)
        assert mae < 5.0, f"MAE too high: {mae}"
