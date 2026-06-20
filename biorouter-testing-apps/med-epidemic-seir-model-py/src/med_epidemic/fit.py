"""Parameter fitting to observed case time-series.

Provides:
- ``grid_search``    — coarse grid search over (β, σ, γ) parameter space
- ``least_squares``  — gradient-free local optimisation (Nelder-Mead)
- ``fit_seir``       — high-level fitting convenience function
- ``fit_sir``        — high-level fitting convenience function for SIR
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, List, Optional, Tuple

import numpy as np

from med_epidemic.models.sir import SIRModel, SIRParams
from med_epidemic.models.seir import SEIRModel, SEIRParams
from med_epidemic.solver import ODESolution


# ---------------------------------------------------------------------------
# Residual / objective
# ---------------------------------------------------------------------------

def _sse(observed: np.ndarray, predicted: np.ndarray) -> float:
    """Sum of squared errors between two arrays (interpolated to common length)."""
    if len(observed) != len(predicted):
        # resample predicted to match observed length
        x_pred = np.linspace(0, 1, len(predicted))
        x_obs = np.linspace(0, 1, len(observed))
        predicted = np.interp(x_obs, x_pred, predicted)
    return float(np.sum((observed - predicted) ** 2))


def _rmse(observed: np.ndarray, predicted: np.ndarray) -> float:
    return float(np.sqrt(np.mean((observed - predicted) ** 2)))


# ---------------------------------------------------------------------------
# Grid search
# ---------------------------------------------------------------------------

@dataclass
class GridSearchResult:
    best_params: dict
    best_score: float
    all_results: list  # list of (params_dict, score)


def grid_search(
    observed_t: np.ndarray,
    observed_I: np.ndarray,
    N: float,
    beta_range: Tuple[float, float, float] = (0.1, 1.0, 5),
    sigma_range: Tuple[float, float, float] = (0.1, 0.5, 3),
    gamma_range: Tuple[float, float, float] = (0.05, 0.5, 3),
    model_type: str = "seir",
    t_span: Tuple[float, float] = (0, 160),
    dt: float = 0.5,
) -> GridSearchResult:
    """Grid search over parameter space.

    Each range is ``(lo, hi, n_points)``.
    """
    betas = np.linspace(*beta_range)
    sigmas = np.linspace(*sigma_range)
    gammas = np.linspace(*gamma_range)

    best_score = float("inf")
    best_params = {}
    all_results = []

    for b in betas:
        for s in sigmas:
            for g in gammas:
                try:
                    if model_type == "sir":
                        params = SIRParams(beta=b, gamma=g, N=N, I0=float(observed_I[0]))
                        model = SIRModel(params)
                    else:
                        params = SEIRParams(
                            beta=b, sigma=s, gamma=g, N=N,
                            E0=0, I0=float(observed_I[0]), R0=0,
                        )
                        model = SEIRModel(params)
                    sol = model.run(t_span=t_span, dt=dt)
                    # extract I trajectory at observed time points
                    i_idx = 1 if model_type == "sir" else 2  # SIR: S=0,I=1,R=2; SEIR: S=0,E=1,I=2,R=3
                    I_pred = np.interp(observed_t, sol.t, sol.y[i_idx])
                    score = _sse(observed_I, I_pred)
                    p = {"beta": b, "gamma": g}
                    if model_type != "sir":
                        p["sigma"] = s
                    all_results.append((p, score))
                    if score < best_score:
                        best_score = score
                        best_params = p.copy()
                except Exception:
                    continue

    return GridSearchResult(best_params=best_params, best_score=best_score, all_results=all_results)


# ---------------------------------------------------------------------------
# Scipy least-squares (Nelder-Mead) — falls back to grid if scipy unavailable
# ---------------------------------------------------------------------------

def least_squares_fit(
    observed_t: np.ndarray,
    observed_I: np.ndarray,
    N: float,
    initial_guess: dict,
    model_type: str = "seir",
    t_span: Tuple[float, float] = (0, 160),
    dt: float = 0.5,
) -> dict:
    """Refine parameters using Nelder-Mead optimisation.

    Falls back to ``scipy.optimize.minimize``; if scipy is not installed,
    returns the initial guess unchanged.
    """
    try:
        from scipy.optimize import minimize
    except ImportError:
        return initial_guess

    i_idx = 1 if model_type == "sir" else 2  # SIR: I=1; SEIR: I=2

    def objective(x):
        if model_type == "sir":
            beta, gamma = x
            params = SIRParams(beta=abs(beta), gamma=abs(gamma), N=N, I0=float(observed_I[0]))
            model = SIRModel(params)
        else:
            beta, sigma, gamma = x
            params = SEIRParams(
                beta=abs(beta), sigma=abs(sigma), gamma=abs(gamma),
                N=N, I0=float(observed_I[0]),
            )
            model = SEIRModel(params)
        try:
            sol = model.run(t_span=t_span, dt=dt)
            I_pred = np.interp(observed_t, sol.t, sol.y[i_idx])
            return _sse(observed_I, I_pred)
        except Exception:
            return 1e12

    if model_type == "sir":
        x0 = np.array([initial_guess["beta"], initial_guess["gamma"]])
    else:
        x0 = np.array([
            initial_guess["beta"],
            initial_guess.get("sigma", 0.3),
            initial_guess["gamma"],
        ])

    res = minimize(objective, x0, method="Nelder-Mead", options={"maxiter": 1000, "xatol": 1e-6})
    if model_type == "sir":
        return {"beta": abs(res.x[0]), "gamma": abs(res.x[1])}
    return {
        "beta": abs(res.x[0]),
        "sigma": abs(res.x[1]),
        "gamma": abs(res.x[2]),
    }


# ---------------------------------------------------------------------------
# High-level fit functions
# ---------------------------------------------------------------------------

def fit_sir(
    observed_t: np.ndarray,
    observed_I: np.ndarray,
    N: float,
    t_span: Tuple[float, float] = (0, 160),
    refine: bool = True,
) -> dict:
    """Fit an SIR model to observed infected counts."""
    grid = grid_search(
        observed_t, observed_I, N,
        model_type="sir", t_span=t_span,
    )
    params = grid.best_params
    if refine:
        params = least_squares_fit(
            observed_t, observed_I, N, params,
            model_type="sir", t_span=t_span,
        )
    return params


def fit_seir(
    observed_t: np.ndarray,
    observed_I: np.ndarray,
    N: float,
    t_span: Tuple[float, float] = (0, 160),
    refine: bool = True,
) -> dict:
    """Fit an SEIR model to observed infected counts."""
    grid = grid_search(
        observed_t, observed_I, N,
        model_type="seir", t_span=t_span,
    )
    params = grid.best_params
    if refine:
        params = least_squares_fit(
            observed_t, observed_I, N, params,
            model_type="seir", t_span=t_span,
        )
    return params
