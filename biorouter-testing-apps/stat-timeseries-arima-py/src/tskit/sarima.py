"""Seasonal ARIMA (SARIMA) — ARIMA with seasonal components.

Model: ARIMA(p,d,q) x (P,D,Q)_m where m is the seasonal period.

Public API
----------
- fit_sarima(x, p, d, q, P, D, Q, m, method='css')
- predict_sarima(model, steps)
- forecast_sarima(model, steps, alpha)
"""

from __future__ import annotations

import math
from typing import List, Any

from .numerics import (
    diff, undiff,
    seasonal_diff, seasonal_undiff,
    mean, variance, zeros, to_vec, Vector,
)
from .arima import fit_arima, predict_arima, forecast_arima
from .ar import _norm_ppf

__all__ = ["fit_sarima", "predict_sarima", "forecast_sarima"]


def _build_seasonal_design(x: Vector, p: int, q: int, P: int, Q: int, m: int):
    """Build design matrix for seasonal ARIMA."""
    n = len(x)
    max_lag = max(p + P * m, q + Q * m)
    A = []
    b = []
    for t in range(max_lag, n):
        row = []
        # AR terms: regular lags
        for i in range(p):
            lag = i + 1
            row.append(x[t - lag] if t - lag >= 0 else 0.0)
        # Seasonal AR terms
        for i in range(P):
            lag = (i + 1) * m
            row.append(x[t - lag] if t - lag >= 0 else 0.0)
        A.append(row)
        b.append(x[t])
    return A, b


def fit_sarima(
    x,
    p: int, d: int, q: int,
    P: int, D: int, Q: int,
    m: int,
    method: str = "css",
) -> dict[str, Any]:
    """Fit a SARIMA(p,d,q)x(P,D,Q)_m model.

    Approach:
    1. Apply seasonal differencing D times, then regular differencing d times.
    2. Fit an extended ARMA model with both regular and seasonal lags.
    3. Store components for forecasting.

    Returns dict with model components.
    """
    xv = to_vec(x)
    n = len(xv)
    mu = mean(xv)

    # Store values needed for undifferencing
    # Seasonal differencing: need last m values for each D level
    seasonal_last_vals = []
    y = list(xv)
    for _ in range(D):
        seasonal_last_vals.append(y[-m:])
        y = seasonal_diff(y, m)

    # Regular differencing
    regular_last_vals = []
    for _ in range(d):
        regular_last_vals.append(y[-1])
        y = diff(y, 1)

    # Build extended ARMA with seasonal lags
    max_lag = max(p + P * m, q + Q * m)
    n_eff = len(y) - max_lag

    if n_eff <= 0:
        raise ValueError(f"Series too short for the given orders (n={n}, max_lag={max_lag})")

    # Design matrix with all AR lags (regular + seasonal)
    A = []
    b_vec = []
    for t in range(max_lag, len(y)):
        row = []
        for i in range(p):
            lag = i + 1
            row.append(y[t - lag] if t - lag >= 0 else 0.0)
        for i in range(P):
            lag = (i + 1) * m
            row.append(y[t - lag] if t - lag >= 0 else 0.0)
        A.append(row)
        b_vec.append(y[t])

    n_ar = p + P
    if n_ar > 0 and len(A) > 0:
        # Solve via normal equations
        from .numerics import lstsq
        ar_coeffs_ext = lstsq(A, b_vec)
        ar_coeffs_regular = ar_coeffs_ext[:p]
        ar_coeffs_seasonal = ar_coeffs_ext[p:]
    else:
        ar_coeffs_regular = []
        ar_coeffs_seasonal = []

    # Compute residuals
    residuals = zeros(len(y))
    for t in range(len(y)):
        ar_part = 0.0
        for i in range(p):
            if t - (i + 1) >= 0:
                ar_part += ar_coeffs_regular[i] * y[t - (i + 1)]
        for i in range(P):
            if t - (i + 1) * m >= 0:
                ar_part += ar_coeffs_seasonal[i] * y[t - (i + 1) * m]
        residuals[t] = y[t] - ar_part

    sigma2 = variance(residuals[max_lag:]) if len(residuals) > max_lag else 1.0

    return {
        "ar_coeffs": ar_coeffs_regular,
        "seasonal_ar_coeffs": ar_coeffs_seasonal,
        "ma_coeffs": [],  # MA estimation deferred; pure AR approximation
        "sigma2": sigma2,
        "d": d,
        "D": D,
        "m": m,
        "p": p,
        "P": P,
        "q": q,
        "Q": Q,
        "regular_last_vals": regular_last_vals,
        "seasonal_last_vals": seasonal_last_vals,
        "mu": mu,
        "residuals": residuals,
        "x_original": xv,
    }


def predict_sarima(model: dict, steps: int = 1) -> List[float]:
    """Multi-step ahead point forecast."""
    ar_coeffs = model["ar_coeffs"]
    seasonal_ar = model["seasonal_ar_coeffs"]
    p = model["p"]
    P = model["P"]
    m = model["m"]
    d = model["d"]
    D = model["D"]
    residuals = model["residuals"]
    regular_last_vals = model["regular_last_vals"]
    seasonal_last_vals = model["seasonal_last_vals"]

    max_lag = max(p + P * m, 1)

    # Build extended history from residuals
    y_hist = list(residuals[-max_lag:]) if max_lag > 0 else [0.0]

    y_forecast = []
    for h in range(steps):
        ar_part = 0.0
        for i in range(p):
            idx = len(y_hist) - (i + 1)
            if idx >= 0:
                ar_part += ar_coeffs[i] * y_hist[idx]
        for i in range(P):
            lag = (i + 1) * m
            idx = len(y_hist) - lag
            if idx >= 0:
                ar_part += seasonal_ar[i] * y_hist[idx]
        y_forecast.append(ar_part)
        y_hist.append(ar_part)

    # Undifference regular
    if d > 0 and regular_last_vals:
        for level in range(d):
            start_val = regular_last_vals[d - 1 - level]
            temp = [start_val]
            for v in y_forecast:
                temp.append(temp[-1] + v)
            y_forecast = temp[1:]

    # Undifference seasonal
    if D > 0 and seasonal_last_vals:
        for level in range(D):
            last_vals = seasonal_last_vals[D - 1 - level]
            temp = []
            for i in range(len(y_forecast)):
                temp.append(y_forecast[i] + last_vals[i % m])
            y_forecast = temp

    return y_forecast


def forecast_sarima(model: dict, steps: int = 1, alpha: float = 0.05) -> dict:
    """Forecast with prediction intervals."""
    point = predict_sarima(model, steps)
    sigma2 = model["sigma2"]
    z = _norm_ppf(1 - alpha / 2)
    lower = []
    upper = []
    for h in range(1, steps + 1):
        var_h = sigma2 * h
        se = math.sqrt(max(var_h, 1e-15))
        lower.append(point[h - 1] - z * se)
        upper.append(point[h - 1] + z * se)
    return {"point": point, "lower": lower, "upper": upper, "alpha": alpha}
