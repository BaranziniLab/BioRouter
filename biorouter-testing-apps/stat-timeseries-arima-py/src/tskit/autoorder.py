"""Automatic order selection via AIC / BIC grid search.

Public API
----------
- auto_arima(x, max_p, max_d, max_q, criterion='aic')
- auto_sarima(x, m, max_p, max_d, max_q, max_P, max_D, max_Q, criterion='aic')
"""

from __future__ import annotations

import math
from typing import List, Tuple, Any

from .numerics import to_vec, variance, Vector
from .arima import fit_arima, predict_arima

__all__ = ["auto_arima", "auto_sarima"]


def _aic_bic(residuals: Vector, n_params: int, n_obs: int, criterion: str = "aic") -> float:
    """Compute AIC or BIC given residuals and number of parameters."""
    sigma2 = variance(residuals) if len(residuals) > n_params else 1e-10
    sigma2 = max(sigma2, 1e-15)
    n = len(residuals)
    ll = -0.5 * n * (math.log(2 * math.pi) + math.log(sigma2) + 1.0)
    k = n_params + 1  # +1 for sigma2
    if criterion == "bic":
        return -2 * ll + k * math.log(n)
    return -2 * ll + 2 * k  # AIC


def auto_arima(
    x,
    max_p: int = 5,
    max_d: int = 2,
    max_q: int = 5,
    criterion: str = "aic",
    verbose: bool = False,
) -> dict[str, Any]:
    """Automatically select ARIMA(p,d,q) orders via grid search.

    Searches over (p,d,q) combinations, fits each, and selects
    the model with the best AIC or BIC.

    Returns dict with best model and selection summary.
    """
    xv = to_vec(x)
    best_score = float("inf")
    best_model = None
    best_order = (0, 0, 0)
    results = []

    for d in range(max_d + 1):
        for p in range(max_p + 1):
            for q in range(max_q + 1):
                if p == 0 and q == 0:
                    continue
                try:
                    model = fit_arima(xv, p, d, q, method="css")
                    n_params = p + q + 1  # +1 for sigma2
                    score = _aic_bic(model["residuals"], n_params, len(xv), criterion)
                    results.append((p, d, q, score))
                    if verbose:
                        print(f"  ARIMA({p},{d},{q}): {criterion.upper()} = {score:.2f}")
                    if score < best_score:
                        best_score = score
                        best_model = model
                        best_order = (p, d, q)
                except Exception as e:
                    if verbose:
                        print(f"  ARIMA({p},{d},{q}): FAILED — {e}")
                    continue

    return {
        "order": best_order,
        "model": best_model,
        "score": best_score,
        "criterion": criterion,
        "results": sorted(results, key=lambda x: x[3]),
    }


def auto_sarima(
    x,
    m: int = 12,
    max_p: int = 3,
    max_d: int = 1,
    max_q: int = 3,
    max_P: int = 1,
    max_D: int = 1,
    max_Q: int = 1,
    criterion: str = "aic",
    verbose: bool = False,
) -> dict[str, Any]:
    """Automatically select SARIMA orders.

    Searches over a reduced grid (seasonal models are expensive).

    Returns dict with best model and selection summary.
    """
    from .sarima import fit_sarima

    xv = to_vec(x)
    best_score = float("inf")
    best_model = None
    best_order = (0, 0, 0, 0, 0, 0)
    results = []

    for d in range(max_d + 1):
        for D in range(max_D + 1):
            for p in range(max_p + 1):
                for q in range(max_q + 1):
                    for P in range(max_P + 1):
                        for Q in range(max_Q + 1):
                            if p == 0 and q == 0 and P == 0 and Q == 0:
                                continue
                            try:
                                model = fit_sarima(xv, p, d, q, P, D, Q, m)
                                n_params = p + q + P + Q + 1
                                score = _aic_bic(model["residuals"], n_params, len(xv), criterion)
                                results.append((p, d, q, P, D, Q, score))
                                if verbose:
                                    print(f"  SARIMA({p},{d},{q})x({P},{D},{Q})_{m}: {criterion.upper()} = {score:.2f}")
                                if score < best_score:
                                    best_score = score
                                    best_model = model
                                    best_order = (p, d, q, P, D, Q)
                            except Exception as e:
                                if verbose:
                                    print(f"  SARIMA({p},{d},{q})x({P},{D},{Q})_{m}: FAILED — {e}")
                                continue

    return {
        "order": best_order,
        "m": m,
        "model": best_model,
        "score": best_score,
        "criterion": criterion,
        "results": sorted(results, key=lambda x: x[6]),
    }
