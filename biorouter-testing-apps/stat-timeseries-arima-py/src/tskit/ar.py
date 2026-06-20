"""Autoregressive (AR) model.

Public API
----------
- fit_yule_walker(x, p)           — AR(p) coefficients via Yule-Walker equations
- fit_least_squares(x, p)         — AR(p) coefficients via least squares
- simulate_ar(coeffs, n, sigma)   — generate AR(p) series
- predict_ar(x, coeffs, steps)    — multi-step ahead forecast
- forecast_ar(x, coeffs, steps, alpha) — forecast with prediction intervals
"""

from __future__ import annotations

import math
from typing import List, Tuple

from .numerics import (
    solve_toeplitz,
    acf as _acf_fn,
    lstsq,
    mean,
    variance,
    zeros,
    randn,
    simulate_ar as _sim_ar,
    to_vec,
    Vector,
)

__all__ = [
    "fit_yule_walker",
    "fit_least_squares",
    "predict_ar",
    "forecast_ar",
    "simulate_ar",
]


# ---------------------------------------------------------------------------
# Fitting
# ---------------------------------------------------------------------------
def fit_yule_walker(x, p: int) -> Tuple[List[float], float]:
    """Estimate AR(p) coefficients via the Yule-Walker equations.

    Returns (coeffs, noise_variance) where coeffs has length p.
    """
    xv = to_vec(x)
    r = _acf_fn(xv, p)
    # Solve Toeplitz system: R a = r[1:p+1]
    coeffs = solve_toeplitz(r[: p + 1])
    # Noise variance estimate
    sig2 = r[0] * (1 - sum(c * r[i + 1] for i, c in enumerate(coeffs)))
    sig2 = max(sig2, 1e-15)
    return coeffs, sig2


def fit_least_squares(x, p: int) -> Tuple[List[float], float]:
    """Estimate AR(p) coefficients via ordinary least squares.

    Returns (coeffs, noise_variance).
    """
    xv = to_vec(x)
    n = len(xv)
    if n <= p:
        raise ValueError(f"Need n > p, got n={n}, p={p}")
    # Design matrix: row t = [x_{t-1}, x_{t-2}, …, x_{t-p}]
    A = []
    b = []
    for t in range(p, n):
        row = [xv[t - 1 - i] for i in range(p)]
        A.append(row)
        b.append(xv[t])
    coeffs = lstsq(A, b)
    # Residual variance
    resid = [b[i] - sum(A[i][j] * coeffs[j] for j in range(p)) for i in range(len(b))]
    sig2 = variance(resid, ddof=p)
    return coeffs, sig2


# ---------------------------------------------------------------------------
# Prediction / forecasting
# ---------------------------------------------------------------------------
def predict_ar(x: Vector, coeffs: List[float], steps: int = 1) -> List[float]:
    """Multi-step ahead point forecast.

    Uses the most recent *p* values from *x* as the seed.
    """
    p = len(coeffs)
    xv = to_vec(x)
    hist = list(xv)  # mutable copy
    forecasts = []
    for _ in range(steps):
        nxt = sum(coeffs[i] * hist[-(i + 1)] for i in range(p))
        forecasts.append(nxt)
        hist.append(nxt)
    return forecasts


def forecast_ar(
    x, coeffs: List[float], steps: int = 1, alpha: float = 0.05,
    sigma2: float | None = None,
) -> dict:
    """Forecast with prediction intervals.

    Returns dict with 'point', 'lower', 'upper', 'alpha'.
    """
    xv = to_vec(x)
    p = len(coeffs)
    if sigma2 is None:
        _, sigma2 = fit_least_squares(xv, p)
    point = predict_ar(xv, coeffs, steps)
    # Build AR representation coefficients psi_j (truncate at max horizon)
    max_lag = steps + p
    psi = zeros(max_lag)
    psi[0] = 1.0
    for j in range(1, max_lag):
        s = 0.0
        if j <= p:
            s += coeffs[j - 1]
        for i in range(1, min(j, p + 1)):
            if j - i < p:
                s += coeffs[j - i - 1] * psi[i - 1]  if i > 0 else 0
        # simpler: psi_j = a_j + sum_{i=1}^{j-1} psi_i * a_{j-i}  (with a_k=0 for k>p)
        s2 = 0.0
        if j <= p:
            s2 += coeffs[j - 1]
        for i in range(1, j):
            ai = coeffs[j - i - 1] if j - i <= p else 0.0
            s2 += psi[i - 1] * ai
        psi[j - 1] = s2 if j > 0 else 1.0

    z = _norm_ppf(1 - alpha / 2)
    lower = []
    upper = []
    for h in range(1, steps + 1):
        # Sum of psi^2 up to h-1
        var_h = sigma2 * sum(psi[j] ** 2 for j in range(h))
        se = math.sqrt(max(var_h, 1e-15))
        lower.append(point[h - 1] - z * se)
        upper.append(point[h - 1] + z * se)
    return {"point": point, "lower": lower, "upper": upper, "alpha": alpha}


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
def simulate_ar(coeffs, n: int = 200, sigma: float = 1.0) -> list[float]:
    """Simulate AR(p) series."""
    return _sim_ar(to_vec(coeffs), n, sigma)


def _norm_ppf(p: float) -> float:
    """Rational approximation to the standard normal quantile (Abramowitz & Stegun 26.2.23)."""
    if p <= 0:
        return -8.0
    if p >= 1:
        return 8.0
    if p == 0.5:
        return 0.0
    if p > 0.5:
        return -_norm_ppf(1 - p)
    t = math.sqrt(-2 * math.log(p))
    c0, c1, c2 = 2.515517, 0.802853, 0.010328
    d1, d2, d3 = 1.432788, 0.189269, 0.001308
    return -(t - (c0 + c1 * t + c2 * t * t) / (1 + d1 * t + d2 * t * t + d3 * t * t * t))
