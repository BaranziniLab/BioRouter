"""Tests for tskit.backtest — rolling-origin backtesting and error metrics."""

import math
from tskit.numerics import set_seed, simulate_ar
from tskit.backtest import mae, rmse, mape, evaluate_forecast, rolling_backtest
from tskit.arima import fit_arima, predict_arima


class TestErrorMetrics:
    def test_mae(self):
        assert mae([1, 2, 3], [1, 2, 3]) == 0.0
        assert mae([0, 0, 0], [1, 2, 3]) == 2.0

    def test_rmse(self):
        assert rmse([1, 2, 3], [1, 2, 3]) == 0.0
        assert rmse([0, 0, 0], [1, 2, 3]) == math.sqrt(14 / 3)

    def test_mape(self):
        assert mape([100, 200], [100, 200]) == 0.0
        # |100-110|/100 = 0.10, |200-180|/200 = 0.10 → mean = 10.0%
        assert mape([100, 200], [110, 180]) == 10.0

    def test_mape_zero_true(self):
        # Zero true values should be skipped
        result = mape([0, 100], [10, 110])
        assert result == 10.0

    def test_evaluate_forecast(self):
        result = evaluate_forecast([1, 2, 3], [1, 2, 3])
        assert result["mae"] == 0.0
        assert result["rmse"] == 0.0
        assert result["mape"] == 0.0


class TestRollingBacktest:
    def test_basic_backtest(self):
        """Backtest on a simple AR(1) series."""
        set_seed(42)
        x = simulate_ar([0.5], n=200, sigma=0.5)

        def fit_fn(train):
            return fit_arima(train, p=1, d=0, q=0)

        def forecast_fn(model, h):
            return predict_arima(model, steps=h)

        bt = rolling_backtest(x, fit_fn, forecast_fn, h=5, min_train=100, step=50)
        assert bt["origins"] > 0
        assert bt["summary"]["mae"] < 5.0  # Should be reasonable
        assert bt["summary"]["rmse"] < 5.0

    def test_backtest_report_error_for_short_series(self):
        """Should raise ValueError for too-short series."""
        x = [1, 2, 3, 4, 5]

        def fit_fn(train):
            return None

        def forecast_fn(model, h):
            return [0.0] * h

        try:
            rolling_backtest(x, fit_fn, forecast_fn, h=5, min_train=100)
            assert False, "Should have raised ValueError"
        except ValueError:
            pass

    def test_coverage_metric(self):
        """Forecast intervals should have reasonable coverage."""
        set_seed(42)
        x = simulate_ar([0.5], n=300, sigma=0.5)
        # Compute in-sample prediction intervals
        from tskit.arima import forecast_arima
        model = fit_arima(x[:250], p=1, d=0, q=0)
        fc = forecast_arima(model, steps=50)
        actual = x[250:300]
        coverage = sum(1 for i in range(50) if fc["lower"][i] <= actual[i] <= fc["upper"][i])
        # 95% intervals should cover roughly 80-100% of points
        assert coverage >= 30, f"Coverage too low: {coverage}/50"
