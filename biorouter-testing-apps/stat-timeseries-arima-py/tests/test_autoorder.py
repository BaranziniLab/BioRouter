"""Tests for tskit.autoorder — automatic order selection."""

from tskit.numerics import set_seed, simulate_ar, diff
from tskit.arima import simulate_arima
from tskit.autoorder import auto_arima


class TestAutoARIMA:
    def test_picks_ar1_on_ar1_data(self):
        """Auto ARIMA should pick p≥1 on AR(1) data."""
        set_seed(42)
        x = simulate_ar([0.7], n=500, sigma=1.0)
        result = auto_arima(x, max_p=3, max_d=1, max_q=3, criterion="aic")
        p, d, q = result["order"]
        assert p >= 1, f"Expected p≥1, got ARIMA({p},{d},{q})"

    def test_picks_d1_on_integrated_data(self):
        """Auto ARIMA should pick d≥1 on integrated data."""
        set_seed(42)
        x = simulate_arima([0.5], [], d=1, n=500)
        result = auto_arima(x, max_p=3, max_d=2, max_q=3, criterion="aic")
        p, d, q = result["order"]
        assert d >= 1, f"Expected d≥1, got ARIMA({p},{d},{q})"

    def test_returns_model(self):
        set_seed(42)
        x = simulate_ar([0.6], n=300)
        result = auto_arima(x, max_p=2, max_d=1, max_q=2)
        assert result["model"] is not None
        assert result["score"] < float("inf")
        assert len(result["results"]) > 0

    def test_bic_vs_aic(self):
        """BIC should tend to select simpler models than AIC."""
        set_seed(42)
        x = simulate_ar([0.5], n=500)
        r_aic = auto_arima(x, max_p=5, max_d=1, max_q=5, criterion="aic")
        r_bic = auto_arima(x, max_p=5, max_d=1, max_q=5, criterion="bic")
        # BIC score should be larger (more penalized)
        assert r_bic["score"] >= r_aic["score"]
