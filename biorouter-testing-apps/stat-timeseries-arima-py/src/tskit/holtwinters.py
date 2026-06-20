"""Holt-Winters exponential smoothing (additive and multiplicative).

Public API
----------
- fit_holt_winters(x, m, method='additive', damped=False)
- predict_hw(model, steps)
- forecast_hw(model, steps, alpha)
"""

from __future__ import annotations

import math
from typing import Any, List

from .numerics import mean, variance, zeros, to_vec, Vector
from .ar import _norm_ppf

__all__ = ["fit_holt_winters", "predict_hw", "forecast_hw"]


def fit_holt_winters(
    x,
    m: int,
    method: str = "additive",
    damped: bool = False,
) -> dict[str, Any]:
    """Fit Holt-Winters exponential smoothing.

    Parameters
    ----------
    x : array-like
        Time series.
    m : int
        Seasonal period.
    method : str
        'additive' or 'multiplicative'.
    damped : bool
        If True, use damped trend.

    Returns
    -------
    dict with keys: level, trend, seasonal, alpha, beta, gamma, m, method, residuals, x_original
    """
    xv = to_vec(x)
    n = len(xv)

    if n < 2 * m:
        raise ValueError(f"Need at least 2*m = {2*m} observations, got {n}")

    method = method.lower()
    is_additive = method == "additive"

    # Initial level, trend, seasonal components
    # Level: mean of first m observations
    level0 = mean(xv[:m])
    # Trend: (mean of second m - mean of first m) / m
    trend0 = (mean(xv[m:2 * m]) - mean(xv[:m])) / m
    # Seasonal: initial seasonal factors
    seasonal0 = []
    for i in range(m):
        if is_additive:
            seasonal0.append(xv[i] - level0)
        else:
            seasonal0.append(xv[i] / level0 if level0 != 0 else 1.0)

    # Initialize smoothing parameters
    alpha = 0.3
    beta = 0.1
    gamma = 0.1
    phi = 0.98 if damped else 1.0

    # Simple grid search over smoothing parameters
    best_sse = float("inf")
    best_params = (alpha, beta, gamma, phi)
    best_components = None

    for a in [0.1, 0.2, 0.3, 0.5, 0.7, 0.9]:
        for b in [0.01, 0.05, 0.1, 0.2]:
            for g in [0.01, 0.05, 0.1, 0.2]:
                for p in [0.98] if damped else [1.0]:
                    lvl, trnd, seas = _hw_fit(xv, m, a, b, g, p, is_additive)
                    sse, residuals = _hw_sse(xv, lvl, trnd, seas, m, p, is_additive)
                    if sse < best_sse:
                        best_sse = sse
                        best_params = (a, b, g, p)
                        best_components = (lvl, trnd, seas)

    alpha, beta, gamma, phi = best_params
    lvl, trnd, seas = best_components

    return {
        "level": lvl,
        "trend": trnd,
        "seasonal": seas,
        "alpha": alpha,
        "beta": beta,
        "gamma": gamma,
        "phi": phi,
        "m": m,
        "method": method,
        "is_additive": is_additive,
        "damped": damped,
        "x_original": xv,
    }


def _hw_fit(x: Vector, m: int, alpha: float, beta: float, gamma: float,
            phi: float, is_additive: bool):
    """One pass of Holt-Winters, returning final level, trend, seasonal arrays."""
    n = len(x)
    # Initial components
    level0 = mean(x[:m])
    trend0 = (mean(x[m:2 * m]) - mean(x[:m])) / m
    seasonal = []
    for i in range(m):
        if is_additive:
            seasonal.append(x[i] - level0)
        else:
            seasonal.append(x[i] / level0 if level0 != 0 else 1.0)

    levels = [level0]
    trends = [trend0]
    # Copy seasonal for updates
    seas = list(seasonal)

    lvl = level0
    trnd = trend0

    for t in range(n):
        s_idx = t % m
        if is_additive:
            new_lvl = alpha * (x[t] - seas[s_idx]) + (1 - alpha) * (lvl + phi * trnd)
            new_trnd = beta * (new_lvl - lvl) + phi * (1 - beta) * trnd
            seas[s_idx] = gamma * (x[t] - new_lvl) + (1 - gamma) * seas[s_idx]
        else:
            denom = lvl + phi * trnd
            if abs(denom) < 1e-15:
                denom = 1e-15
            new_lvl = alpha * (x[t] / seas[s_idx]) + (1 - alpha) * (lvl + phi * trnd)
            new_trnd = beta * (new_lvl - lvl) + phi * (1 - beta) * trnd
            if abs(new_lvl) < 1e-15:
                new_lvl = 1e-15
            seas[s_idx] = gamma * (x[t] / new_lvl) + (1 - gamma) * seas[s_idx]
        lvl = new_lvl
        trnd = new_trnd
        levels.append(lvl)
        trends.append(trnd)

    return levels, trends, seas


def _hw_sse(x: Vector, levels, trends, seas, m, phi, is_additive):
    """Compute SSE and residuals for given HW components."""
    n = len(x)
    fitted = []
    for t in range(n):
        s_idx = t % m
        if is_additive:
            f = levels[t] + phi * trends[t] + seas[s_idx]
        else:
            f = (levels[t] + phi * trends[t]) * seas[s_idx]
        fitted.append(f)
    residuals = [x[t] - fitted[t] for t in range(n)]
    sse = sum(r * r for r in residuals)
    return sse, residuals


def predict_hw(model: dict, steps: int = 1) -> List[float]:
    """Multi-step ahead point forecast."""
    lvl = model["level"][-1]
    trnd = model["trend"][-1]
    seas = model["seasonal"]
    m = model["m"]
    is_additive = model["is_additive"]
    phi = model["phi"]

    forecasts = []
    for h in range(1, steps + 1):
        # Seasonal index wraps around
        s_idx = (len(model["x_original"]) + h - 1) % m
        # Damped trend: phi + phi^2 + ... + phi^h
        if model["damped"]:
            trend_sum = sum(phi ** i for i in range(1, h + 1))
        else:
            trend_sum = h
        if is_additive:
            f = lvl + trend_sum * trnd + seas[s_idx]
        else:
            f = (lvl + trend_sum * trnd) * seas[s_idx]
        forecasts.append(f)
    return forecasts


def forecast_hw(model: dict, steps: int = 1, alpha: float = 0.05) -> dict:
    """Forecast with prediction intervals.

    Uses residual-based variance estimation.
    """
    point = predict_hw(model, steps)
    # Estimate residual variance
    x = model["x_original"]
    m = model["m"]
    lvl = model["level"]
    trnd = model["trend"]
    seas = model["seasonal"]
    is_additive = model["is_additive"]
    phi = model["phi"]

    fitted = []
    for t in range(len(x)):
        s_idx = t % m
        if is_additive:
            f = lvl[t] + phi * trnd[t] + seas[s_idx]
        else:
            f = (lvl[t] + phi * trnd[t]) * seas[s_idx]
        fitted.append(f)
    residuals = [x[t] - fitted[t] for t in range(len(x))]
    sigma2 = variance(residuals)

    z = _norm_ppf(1 - alpha / 2)
    lower = []
    upper = []
    for h in range(1, steps + 1):
        # Variance grows roughly linearly with horizon for HW
        var_h = sigma2 * h
        se = math.sqrt(max(var_h, 1e-15))
        lower.append(point[h - 1] - z * se)
        upper.append(point[h - 1] + z * se)
    return {"point": point, "lower": lower, "upper": upper, "alpha": alpha}
