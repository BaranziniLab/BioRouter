"""Moving-Average (MA) model.

Public API
----------
- fit_ma_css(x, q)        — MA(q) coefficients via conditional sum of squares
- fit_ma_mle(x, q)        — MA(q) coefficients via approximate MLE (Nelder-Mead)
- predict_ma(resid, coeffs, steps)  — forecast from MA model
- forecast_ma(x, coeffs, steps, alpha, sigma2)
- simulate_ma(coeffs, n, sigma)
"""

from __future__ import annotations

import math
from typing import List, Tuple

from .numerics import (
    mean,
    variance,
    zeros,
    simulate_ma as _sim_ma,
    to_vec,
    Vector,
)

__all__ = [
    "fit_ma_css",
    "fit_ma_mle",
    "predict_ma",
    "forecast_ma",
    "simulate_ma",
]


# ---------------------------------------------------------------------------
# CSS fitting
# ---------------------------------------------------------------------------
def fit_ma_css(x, q: int, maxiter: int = 200, tol: float = 1e-8) -> Tuple[List[float], float]:
    """Fit MA(q) via conditional sum-of-squares with gradient-free optimisation.

    Uses a simple coordinate-descent on the innovation likelihood.
    Returns (coeffs, sigma2).
    """
    xv = to_vec(x)
    n = len(xv)
    mu = mean(xv)
    yc = [xi - mu for xi in xv]
    theta = zeros(q)
    sigma2 = variance(yc)
    if sigma2 == 0:
        return theta, 0.0

    for _ in range(maxiter):
        # Compute innovations eps_t = y_t - sum(theta_i * eps_{t-i})
        eps = zeros(n)
        for t in range(n):
            s = 0.0
            for i in range(q):
                if t - 1 - i >= 0:
                    s += theta[i] * eps[t - 1 - i]
            eps[t] = yc[t] - s
        # CSS objective
        ss = sum(e * e for e in eps[q:])  # conditional on first q
        old_ss = ss
        # Gradient-free: try small perturbation for each coefficient
        for j in range(q):
            best_theta = list(theta)
            for delta in [-0.05, 0.05]:
                trial = list(theta)
                trial[j] += delta
                eps2 = zeros(n)
                for t in range(n):
                    s = 0.0
                    for i in range(q):
                        if t - 1 - i >= 0:
                            s += trial[i] * eps2[t - 1 - i]
                    eps2[t] = yc[t] - s
                ss2 = sum(e * e for e in eps2[q:])
                if ss2 < ss:
                    ss = ss2
                    best_theta = trial
            theta = best_theta
        # Check convergence
        if abs(ss - old_ss) < tol * max(abs(old_ss), 1.0):
            break
        sigma2 = ss / (n - q) if n > q else ss
    # Final residuals
    eps = zeros(n)
    for t in range(n):
        s = 0.0
        for i in range(q):
            if t - 1 - i >= 0:
                s += theta[i] * eps[t - 1 - i]
        eps[t] = yc[t] - s
    sigma2 = sum(e * e for e in eps[q:]) / (n - q) if n > q else 1.0
    return theta, sigma2


# ---------------------------------------------------------------------------
# Approximate MLE via Nelder-Mead simplex
# ---------------------------------------------------------------------------
def fit_ma_mle(x, q: int, maxiter: int = 500) -> Tuple[List[float], float]:
    """Fit MA(q) via approximate MLE using Nelder-Mead."""
    xv = to_vec(x)
    n = len(xv)
    mu = mean(xv)
    yc = [xi - mu for xi in xv]

    def neg_loglik(params):
        theta = params[:q]
        log_sigma2 = params[q]
        sigma2 = math.exp(log_sigma2)
        eps = zeros(n)
        for t in range(n):
            s = 0.0
            for i in range(q):
                if t - 1 - i >= 0:
                    s += theta[i] * eps[t - 1 - i]
            eps[t] = yc[t] - s
        ss = sum(e * e for e in eps[q:])
        nl = 0.5 * (n - q) * math.log(sigma2) + ss / (2 * sigma2)
        return nl

    # Simplex initialisation
    x0 = zeros(q + 1)
    x0[q] = math.log(max(variance(yc), 1e-10))
    simplex = [x0]
    for i in range(q + 1):
        pt = list(x0)
        pt[i] += 0.1
        simplex.append(pt)
    vals = [neg_loglik(s) for s in simplex]

    for _ in range(maxiter):
        # Find worst
        idx_w = max(range(len(simplex)), key=lambda i: vals[i])
        idx_b = min(range(len(simplex)), key=lambda i: vals[i])
        centroid = zeros(q + 1)
        for i, s in enumerate(simplex):
            if i != idx_w:
                for j in range(q + 1):
                    centroid[j] += s[j]
        for j in range(q + 1):
            centroid[j] /= q + 1
        # Reflect
        reflect = [2 * centroid[j] - simplex[idx_w][j] for j in range(q + 1)]
        rv = neg_loglik(reflect)
        if rv < vals[idx_b]:
            # Expand
            expand = [2 * reflect[j] - centroid[j] for j in range(q + 1)]
            ev = neg_loglik(expand)
            if ev < rv:
                simplex[idx_w] = expand
                vals[idx_w] = ev
            else:
                simplex[idx_w] = reflect
                vals[idx_w] = rv
        elif rv < max(vals[i] for i in range(len(simplex)) if i != idx_w):
            simplex[idx_w] = reflect
            vals[idx_w] = rv
        else:
            # Contract
            best = simplex[idx_b]
            contracted = [0.5 * (best[j] + simplex[idx_w][j]) for j in range(q + 1)]
            cv = neg_loglik(contracted)
            if cv < vals[idx_w]:
                simplex[idx_w] = contracted
                vals[idx_w] = cv
            else:
                # Shrink toward best
                for i in range(len(simplex)):
                    if i != idx_b:
                        simplex[i] = [0.5 * (simplex[i][j] + best[j]) for j in range(q + 1)]
                        vals[i] = neg_loglik(simplex[i])
        # Convergence check
        spread = max(vals) - min(vals)
        if spread < 1e-10:
            break

    best = simplex[min(range(len(vals)), key=lambda i: vals[i])]
    theta = best[:q]
    sigma2 = math.exp(best[q])
    return theta, sigma2


# ---------------------------------------------------------------------------
# Forecasting
# ---------------------------------------------------------------------------
def predict_ma(eps: Vector, coeffs: List[float], steps: int = 1) -> List[float]:
    """Point forecast from MA model using historical residuals."""
    q = len(coeffs)
    forecasts = []
    for h in range(1, steps + 1):
        if h <= q:
            forecasts.append(coeffs[h - 1] * eps[-1] if h == 1 else 0.0)
            # Only immediate shock matters; later shocks are E[eps]=0
            # Actually: E[X_{n+h}] = sum_{i} theta_i * E[eps_{n+h-i}]
            # For h>q, all terms have E[eps]=0 so forecast=0.
        else:
            forecasts.append(0.0)
    return forecasts


def forecast_ma(
    x, coeffs: List[float], steps: int = 1, alpha: float = 0.05,
    sigma2: float | None = None,
) -> dict:
    """Forecast with prediction intervals.

    Returns dict with 'point', 'lower', 'upper', 'alpha'.
    """
    from .ar import _norm_ppf

    xv = to_vec(x)
    q = len(coeffs)
    mu = mean(xv)
    # Compute residuals
    n = len(xv)
    eps = zeros(n)
    yc = [xi - mu for xi in xv]
    for t in range(n):
        s = 0.0
        for i in range(q):
            if t - 1 - i >= 0:
                s += coeffs[i] * eps[t - 1 - i]
        eps[t] = yc[t] - s

    if sigma2 is None:
        sigma2 = variance(eps[q:]) if n > q else 1.0

    # MA representation: forecast variance for h-step ahead
    z = _norm_ppf(1 - alpha / 2)
    point = []
    lower = []
    upper = []
    for h in range(1, steps + 1):
        if h <= q:
            pt = mu + coeffs[h - 1] * eps[-1]
            var_h = sigma2 * (1 + sum(coeffs[i] ** 2 for i in range(h - 1)))
        else:
            pt = mu
            var_h = sigma2 * (1 + sum(c ** 2 for c in coeffs))
        se = math.sqrt(max(var_h, 1e-15))
        point.append(pt)
        lower.append(pt - z * se)
        upper.append(pt + z * se)
    return {"point": point, "lower": lower, "upper": upper, "alpha": alpha}


# ---------------------------------------------------------------------------
# Simulation
# ---------------------------------------------------------------------------
def simulate_ma(coeffs, n: int = 200, sigma: float = 1.0) -> list[float]:
    """Simulate MA(q) series."""
    return _sim_ma(to_vec(coeffs), n, sigma)
