"""Tests for tskit.sarima — seasonal ARIMA."""

from tskit.numerics import set_seed, mean
from tskit.sarima import fit_sarima, predict_sarima, forecast_sarima


def _make_seasonal_series(n=120, m=12, trend=0.1, seasonal_amp=5.0):
    """Create a deterministic seasonal series with trend."""
    import math
    x = []
    for i in range(n):
        val = trend * i + seasonal_amp * math.sin(2 * math.pi * i / m)
        x.append(val)
    return x


class TestSARIMAFit:
    def test_fit_structure(self):
        """Fit SARIMA(1,0,0)x(1,0,0)_12 and check output."""
        x = _make_seasonal_series(120, m=12)
        model = fit_sarima(x, p=1, d=0, q=0, P=1, D=0, Q=0, m=12)
        assert model["m"] == 12
        assert model["p"] == 1
        assert model["P"] == 1
        assert model["sigma2"] > 0

    def test_fit_with_differencing(self):
        """SARIMA with seasonal and regular differencing."""
        x = _make_seasonal_series(120, m=12, trend=0.1)
        model = fit_sarima(x, p=1, d=1, q=0, P=1, D=1, Q=0, m=12)
        assert model["d"] == 1
        assert model["D"] == 1


class TestSARIMAForecast:
    def test_forecast_structure(self):
        x = _make_seasonal_series(120, m=12)
        model = fit_sarima(x, p=1, d=0, q=0, P=1, D=0, Q=0, m=12)
        fc = forecast_sarima(model, steps=12, alpha=0.05)
        assert len(fc["point"]) == 12
        assert len(fc["lower"]) == 12
        assert len(fc["upper"]) == 12

    def test_forecast_intervals_ordered(self):
        x = _make_seasonal_series(120, m=12)
        model = fit_sarima(x, p=1, d=0, q=0, P=1, D=0, Q=0, m=12)
        fc = forecast_sarima(model, steps=5)
        for i in range(5):
            assert fc["lower"][i] <= fc["point"][i] <= fc["upper"][i]
