"""ARIMA(p,d,q) model — differencing + ARMA estimation.

Public API
----------
- fit_arima(x, p, d, q, method='css')  — fit ARIMA model
- predict_arima(model, steps)           — multi-step forecast
- forecast_arima(model, steps, alpha)   — forecast with prediction intervals
- simulate_arima(ar, ma, d, n, sigma)   — generate ARIMA series
"""

from __future__ import annotations

import math
from typing import List, Tuple, Any

from .numerics import diff, undiff, arima_seeds, mean, variance, zeros, to_vec, Vector
from .ar import fit_yule_walker, fit_least_squares, predict_ar, _norm_ppf
from .ma import fit_ma_css, fit_ma_mle

__all__ = ["fit_arima", "predict_arima", "forecast_arima", "simulate_arima"]


def fit_arima(
    x,
    p: int,
    d: int,
    q: int,
    method: str = "css",
) -> dict[str, Any]:
    """Fit an ARIMA(p,d,q) model.

    Steps:
    1. Difference the series d times to achieve stationarity.
    2. Fit ARMA(p,q) on the differenced series.
    3. Store components for forecasting.

    Parameters
    ----------
    x : array-like
        Time series data.
    p : int
        AR order.
    d : int
        Differencing order.
    q : int
        MA order.
    method : str
        'css' for conditional sum-of-squares, 'mle' for approximate MLE.

    Returns
    -------
    dict with keys: ar_coeffs, ma_coeffs, sigma2, d, intercept, residuals, x_original, mu
    """
    xv = to_vec(x)
    n = len(xv)
    mu = mean(xv)

    # Step 1: Difference
    y = diff(xv, d) if d > 0 else list(xv)
    # Store first d values as seeds for undifferencing
    seeds = arima_seeds(xv, d) if d > 0 else []

    # Step 2: Fit ARMA(p,q) on differenced series
    if p == 0 and q == 0:
        ar_coeffs = []
        ma_coeffs = []
        sigma2 = variance(y) if y else 1.0
    elif p > 0 and q == 0:
        # Pure AR
        if method == "yule_walker":
            ar_coeffs, sigma2 = fit_yule_walker(y, p)
        else:
            ar_coeffs, sigma2 = fit_least_squares(y, p)
        ma_coeffs = []
    elif p == 0 and q > 0:
        # Pure MA
        if method == "mle":
            ma_coeffs, sigma2 = fit_ma_mle(y, q)
        else:
            ma_coeffs, sigma2 = fit_ma_css(y, q)
        ar_coeffs = []
    else:
        # ARMA(p,q) — iterate between AR and MA estimation
        ar_coeffs, sigma2 = fit_least_squares(y, p)
        ma_coeffs = zeros(q)
        # Iterate
        for _ in range(10):
            # Compute residuals with current AR + MA
            residuals = zeros(len(y))
            for t in range(len(y)):
                ar_part = sum(ar_coeffs[i] * y[t - 1 - i] for i in range(p) if t - 1 - i >= 0)
                ma_part = sum(ma_coeffs[i] * residuals[t - 1 - i] for i in range(q) if t - 1 - i >= 0)
                residuals[t] = y[t] - ar_part - ma_part
            # Re-estimate AR given residuals
            if p > 0:
                new_ar, _ = fit_least_squares(y, p)
                # Update AR
                ar_coeffs = new_ar
            # Re-estimate MA given AR
            if q > 0:
                # Recompute residuals
                residuals2 = zeros(len(y))
                for t in range(len(y)):
                    ar_part = sum(ar_coeffs[i] * y[t - 1 - i] for i in range(p) if t - 1 - i >= 0)
                    ma_part = sum(ma_coeffs[i] * residuals2[t - 1 - i] for i in range(q) if t - 1 - i >= 0)
                    residuals2[t] = y[t] - ar_part - ma_part
                # Fit MA on residuals (treat as MA process on residuals)
                ma_new, sigma2_new = fit_ma_css(residuals2, q)
                # Check convergence
                if all(abs(ma_new[i] - ma_coeffs[i]) < 1e-8 for i in range(q)):
                    break
                ma_coeffs = ma_new
                sigma2 = sigma2_new

    # Compute final residuals
    residuals = zeros(len(y))
    for t in range(len(y)):
        ar_part = sum(ar_coeffs[i] * y[t - 1 - i] for i in range(p) if t - 1 - i >= 0)
        ma_part = sum(ma_coeffs[i] * residuals[t - 1 - i] for i in range(q) if t - 1 - i >= 0)
        residuals[t] = y[t] - ar_part - ma_part

    return {
        "ar_coeffs": ar_coeffs,
        "ma_coeffs": ma_coeffs,
        "sigma2": sigma2,
        "d": d,
        "seeds": seeds,
        "mu": mu,
        "residuals": residuals,
        "x_original": xv,
        "p": p,
        "q": q,
    }


def predict_arima(model: dict, steps: int = 1) -> List[float]:
    """Multi-step ahead point forecast."""
    d = model["d"]
    ar_coeffs = model["ar_coeffs"]
    ma_coeffs = model["ma_coeffs"]
    residuals = model["residuals"]
    seeds = model["seeds"]

    # Forecast on differenced series
    if len(ar_coeffs) == 0 and len(ma_coeffs) == 0:
        # White noise — forecast is 0
        y_forecast = [0.0] * steps
    else:
        # Use AR representation
        p = len(ar_coeffs)
        q = len(ma_coeffs)
        max_lag = max(p, q)
        # Build history: append zeros for future
        y_hist = list(residuals[-max_lag:]) if max_lag > 0 else []
        y_forecast = []
        for h in range(steps):
            ar_part = sum(ar_coeffs[i] * (y_hist[-(i + 1)] if i < len(y_hist) else 0.0) for i in range(p))
            # MA part: future residuals are 0
            ma_part = 0.0
            y_hat = ar_part + ma_part
            y_forecast.append(y_hat)
            y_hist.append(y_hat)
    # Undifference: integrate forecasts from the last observed value
    if d > 0:
        xv = model["x_original"]
        last_val = xv[-1]
        forecast = [last_val]
        for v in y_forecast:
            forecast.append(forecast[-1] + v)
        forecast = forecast[1:]  # Remove the seed value
    else:
        forecast = y_forecast
    return forecast


def forecast_arima(
    model: dict,
    steps: int = 1,
    alpha: float = 0.05,
) -> dict:
    """Forecast with prediction intervals.

    Returns dict with 'point', 'lower', 'upper', 'alpha'.
    """
    point = predict_arima(model, steps)
    sigma2 = model["sigma2"]
    z = _norm_ppf(1 - alpha / 2)
    # Rough approximation: variance grows with horizon
    lower = []
    upper = []
    for h in range(1, steps + 1):
        # Approximate forecast variance (ARIMA with differencing has increasing variance)
        var_h = sigma2 * h
        se = math.sqrt(max(var_h, 1e-15))
        lower.append(point[h - 1] - z * se)
        upper.append(point[h - 1] + z * se)
    return {"point": point, "lower": lower, "upper": upper, "alpha": alpha}


def simulate_arima(
    ar_coeffs: List[float],
    ma_coeffs: List[float],
    d: int,
    n: int = 200,
    sigma: float = 1.0,
) -> list[float]:
    """Simulate ARIMA(p,d,q) series.

    First simulate ARMA(p,q), then integrate d times.
    """
    p = len(ar_coeffs)
    q = len(ma_coeffs)
    # Simulate ARMA
    max_lag = max(p, q) * 4 + n
    eps = [sigma * (sum([1.0]) * 0.0) for _ in range(max_lag)]  # placeholder
    from .numerics import randn
    eps = [sigma * randn() for _ in range(max_lag)]
    x = zeros(max_lag)
    for t in range(max_lag):
        ar_part = sum(ar_coeffs[i] * x[t - 1 - i] for i in range(p) if t - 1 - i >= 0)
        ma_part = sum(ma_coeffs[i] * eps[t - 1 - i] for i in range(q) if t - 1 - i >= 0)
        x[t] = ar_part + ma_part + eps[t]
    # Take last n values
    arma_series = x[max_lag - n:]
    # Integrate d times
    result = list(arma_series)
    for _ in range(d):
        integrated = [0.0] * len(result)
        integrated[0] = result[0]  # initial value
        for i in range(1, len(result)):
            integrated[i] = integrated[i - 1] + result[i]
        result = integrated
    return result
