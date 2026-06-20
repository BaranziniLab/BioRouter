"""Stochastic epidemic simulation via Gillespie's Stochastic Simulation Algorithm (SSA).

The Gillespie SSA exactly simulates the continuous-time Markov chain
that underlies a compartmental epidemic model in a finite population of
size *N*.

Implements SIR, SEIR, and SEIRD stochastic models with the same API as
the deterministic counterparts (``.run()`` returns sampled trajectories).
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import List, Optional, Tuple

import numpy as np


# ---------------------------------------------------------------------------
# Core Gillespie engine
# ---------------------------------------------------------------------------

def gillespie_ssa(
    propensities_fn,
    state_change_matrix,
    y0: np.ndarray,
    t_span: Tuple[float, float] = (0, 200),
    rng: Optional[np.random.Generator] = None,
) -> Tuple[np.ndarray, np.ndarray]:
    """Run the Gillespie SSA.

    Parameters
    ----------
    propensities_fn : callable(y) -> np.ndarray
        Returns a vector of reaction propensities.
    state_change_matrix : np.ndarray, shape (n_reactions, n_states)
        Each row is the state change vector for one reaction.
    y0 : np.ndarray
        Initial integer state vector.
    t_span : (t0, tf)
    rng : np.random.Generator, optional

    Returns
    -------
    t_out, y_out : arrays of sampled time points and states.
    """
    rng = rng or np.random.default_rng()
    t = t_span[0]
    tf = t_span[1]
    y = y0.copy().astype(int)

    t_list: List[float] = [t]
    y_list: List[np.ndarray] = [y.copy()]

    while t < tf:
        props = propensities_fn(y)
        total = props.sum()
        if total <= 0:
            break  # no more events possible

        # time to next event (exponential)
        tau = rng.exponential(1.0 / total)
        if t + tau > tf:
            break
        t += tau

        # which reaction fires
        reaction_idx = rng.choice(len(props), p=props / total)
        y = y + state_change_matrix[reaction_idx].astype(int)

        t_list.append(t)
        y_list.append(y.copy())

    return np.array(t_list), np.column_stack(y_list)


# ---------------------------------------------------------------------------
# SIR Gillespie
# ---------------------------------------------------------------------------

def _sir_propensities(y: np.ndarray, beta: float, gamma: float, N: int) -> np.ndarray:
    S, I, R = int(y[0]), int(y[1]), int(y[2])
    infection = beta * S * I / N
    recovery = gamma * I
    return np.array([infection, recovery])


_SIR_SCM = np.array([
    [-1, 1, 0],  # infection
    [0, -1, 1],  # recovery
])


def run_sir_gillespie(
    N: int,
    beta: float,
    gamma: float,
    I0: int = 1,
    t_span: Tuple[float, float] = (0, 200),
    rng: Optional[np.random.Generator] = None,
) -> Tuple[np.ndarray, np.ndarray]:
    """Run stochastic SIR via Gillespie SSA."""
    y0 = np.array([N - I0, I0, 0])
    prop = lambda y: _sir_propensities(y, beta, gamma, N)
    return gillespie_ssa(prop, _SIR_SCM, y0, t_span, rng=rng)


# ---------------------------------------------------------------------------
# SEIR Gillespie
# ---------------------------------------------------------------------------

def _seir_propensities(y, beta, sigma, gamma, N):
    S, E, I, R = int(y[0]), int(y[1]), int(y[2]), int(y[3])
    return np.array([
        beta * S * I / N,   # infection
        sigma * E,           # progression
        gamma * I,           # recovery
    ])


_SEIR_SCM = np.array([
    [-1, 1, 0, 0],  # infection
    [0, -1, 1, 0],  # E → I
    [0, 0, -1, 1],  # recovery
])


def run_seir_gillespie(
    N: int,
    beta: float,
    sigma: float,
    gamma: float,
    I0: int = 1,
    E0: int = 0,
    t_span: Tuple[float, float] = (0, 200),
    rng: Optional[np.random.Generator] = None,
) -> Tuple[np.ndarray, np.ndarray]:
    """Run stochastic SEIR via Gillespie SSA."""
    y0 = np.array([N - E0 - I0, E0, I0, 0])
    prop = lambda y: _seir_propensities(y, beta, sigma, gamma, N)
    return gillespie_ssa(prop, _SEIR_SCM, y0, t_span, rng=rng)


# ---------------------------------------------------------------------------
# SEIRD Gillespie
# ---------------------------------------------------------------------------

def _seird_propensities(y, beta, sigma, gamma, mu, N):
    S, E, I, R, D = int(y[0]), int(y[1]), int(y[2]), int(y[3]), int(y[4])
    return np.array([
        beta * S * I / N,
        sigma * E,
        gamma * I,
        mu * I,
    ])


_SEIRD_SCM = np.array([
    [-1, 1, 0, 0, 0],
    [0, -1, 1, 0, 0],
    [0, 0, -1, 1, 0],
    [0, 0, -1, 0, 1],
])


def run_seird_gillespie(
    N: int,
    beta: float,
    sigma: float,
    gamma: float,
    mu: float,
    I0: int = 1,
    E0: int = 0,
    t_span: Tuple[float, float] = (0, 200),
    rng: Optional[np.random.Generator] = None,
) -> Tuple[np.ndarray, np.ndarray]:
    """Run stochastic SEIRD via Gillespie SSA."""
    y0 = np.array([N - E0 - I0, E0, I0, 0, 0])
    prop = lambda y: _seird_propensities(y, beta, sigma, gamma, mu, N)
    return gillespie_ssa(prop, _SEIRD_SCM, y0, t_span, rng=rng)


# ---------------------------------------------------------------------------
# Ensemble helper
# ---------------------------------------------------------------------------

def run_ensemble(
    sim_fn,
    n_runs: int,
    seed: int = 42,
    **kwargs,
) -> List[Tuple[np.ndarray, np.ndarray]]:
    """Run *n_runs* stochastic simulations, returning a list of (t, y) tuples."""
    results = []
    for i in range(n_runs):
        rng = np.random.default_rng(seed + i)
        t, y = sim_fn(rng=rng, **kwargs)
        results.append((t, y))
    return results


def ensemble_mean(
    trajectories: list,
    n_states: int,
    n_time_points: int = 500,
) -> Tuple[np.ndarray, np.ndarray]:
    """Interpolate ensemble trajectories onto a common time grid and return mean.

    Parameters
    ----------
    trajectories : list of (t, y) tuples
    n_states : number of compartments
    n_time_points : resolution of the output grid

    Returns
    -------
    t_grid, y_mean : common time axis and mean state values
    """
    # build common time grid
    t_max = max(t.max() for t, _ in trajectories)
    t_grid = np.linspace(0, t_max, n_time_points)
    accum = np.zeros((n_states, n_time_points))

    for t, y in trajectories:
        for s in range(n_states):
            accum[s] += np.interp(t_grid, t, y[s])

    y_mean = accum / len(trajectories)
    return t_grid, y_mean
