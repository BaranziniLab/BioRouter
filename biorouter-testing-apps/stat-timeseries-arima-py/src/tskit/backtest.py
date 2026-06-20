"""Backtesting (rolling-origin) with error metrics.

Public API
----------
- rolling_backtest(x, fit_fn, h, min_train, step)
- mae(y_true, y_pred)
- rmse(y_true, y_pred)
- mape(y_true, y_pred)
- evaluate_forecast(y_true, y_pred)  → dict of all metrics
"""

from __future__ import annotations

from typing import Callable, List, Any

from .numerics import to_vec, Vector

__all__ = ["rolling_backtest", "mae", "rmse", "mape", "evaluate_forecast"]


def mae(y_true: Vector, y_pred: Vector) -> float:
    """Mean Absolute Error."""
    n = min(len(y_true), len(y_pred))
    return sum(abs(y_true[i] - y_pred[i]) for i in range(n)) / n


def rmse(y_true: Vector, y_pred: Vector) -> float:
    """Root Mean Squared Error."""
    import math
    n = min(len(y_true), len(y_pred))
    return math.sqrt(sum((y_true[i] - y_pred[i]) ** 2 for i in range(n)) / n)


def mape(y_true: Vector, y_pred: Vector) -> float:
    """Mean Absolute Percentage Error (ignores zero true values)."""
    n = min(len(y_true), len(y_pred))
    total = 0.0
    count = 0
    for i in range(n):
        if abs(y_true[i]) > 1e-10:
            total += abs((y_true[i] - y_pred[i]) / y_true[i])
            count += 1
    return (total / count * 100.0) if count > 0 else float("inf")


def evaluate_forecast(y_true: Vector, y_pred: Vector) -> dict[str, float]:
    """Compute all error metrics at once."""
    return {
        "mae": mae(y_true, y_pred),
        "rmse": rmse(y_true, y_pred),
        "mape": mape(y_true, y_pred),
    }


def rolling_backtest(
    x,
    fit_fn: Callable[[List[float]], Any],
    forecast_fn: Callable[[Any, int], Vector],
    h: int = 1,
    min_train: int | None = None,
    step: int = 1,
) -> dict[str, Any]:
    """Rolling-origin backtest.

    Parameters
    ----------
    x : array-like
        Full time series.
    fit_fn : callable
        ``fit_fn(train_series) → model`` — fits a model on the training window.
    forecast_fn : callable
        ``forecast_fn(model, h) → list[float]`` — produces h-step-ahead forecasts.
    h : int
        Forecast horizon.
    min_train : int or None
        Minimum training window size. Default: max(30, 2*h).
    step : int
        Step size between origins.

    Returns
    -------
    dict with:
      errors — list of dicts per origin (actual, predicted, metrics)
      summary — aggregated metrics
      origins — number of origins tested
    """
    xv = to_vec(x)
    n = len(xv)
    if min_train is None:
        min_train = max(30, 2 * h)

    if n < min_train + h:
        raise ValueError(
            f"Series length {n} too short for min_train={min_train} and h={h}"
        )

    all_errors = []
    origins = 0

    for t in range(min_train, n - h + 1, step):
        train = xv[:t]
        actual = xv[t : t + h]
        try:
            model = fit_fn(train)
            pred = forecast_fn(model, h)
            metrics = evaluate_forecast(actual, pred)
            all_errors.append({
                "origin": t,
                "actual": actual,
                "predicted": pred,
                **metrics,
            })
            origins += 1
        except Exception:
            continue

    if origins == 0:
        return {
            "errors": [],
            "summary": {"mae": float("inf"), "rmse": float("inf"), "mape": float("inf")},
            "origins": 0,
        }

    # Aggregate
    avg_mae = sum(e["mae"] for e in all_errors) / origins
    avg_rmse = sum(e["rmse"] for e in all_errors) / origins
    avg_mape = sum(e["mape"] for e in all_errors) / origins

    return {
        "errors": all_errors,
        "summary": {"mae": avg_mae, "rmse": avg_rmse, "mape": avg_mape},
        "origins": origins,
    }
