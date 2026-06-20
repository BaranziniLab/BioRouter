"""Epidemic summary metrics.

Provides:
- ``compute_R0``  — basic reproduction number (model parameters)
- ``compute_Rt``  — effective Rt over time from a trajectory
- ``peak_infections`` — peak count and timing of the I compartment
- ``attack_rate`` — total fraction of the population ever infected
- ``final_size``  — total recovered + dead at end
- ``epidemic_duration`` — time from start to when I drops below threshold
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional

import numpy as np

from med_epidemic.solver import ODESolution


@dataclass
class EpidemicMetrics:
    """Container for computed epidemic summary statistics."""

    R0: float
    peak_infected: float
    peak_time: float
    attack_rate: float
    final_size: float
    total_pop: float
    epidemic_duration: Optional[float] = None

    def summary_dict(self) -> dict:
        return {
            "R0": round(self.R0, 4),
            "peak_infected": round(self.peak_infected, 2),
            "peak_time (days)": round(self.peak_time, 2),
            "attack_rate": round(self.attack_rate, 4),
            "final_size": round(self.final_size, 2),
            "total_pop": round(self.total_pop, 2),
            "epidemic_duration (days)": (
                round(self.epidemic_duration, 2)
                if self.epidemic_duration is not None
                else None
            ),
        }


# ---------------------------------------------------------------------------
# Basic reproduction number
# ---------------------------------------------------------------------------

def compute_R0(beta: float, gamma: float) -> float:
    """R₀ = β / γ for SIR-type models."""
    if gamma <= 0:
        return float("inf")
    return beta / gamma


# ---------------------------------------------------------------------------
# Effective Rt over time
# ---------------------------------------------------------------------------

def compute_Rt(
    solution: ODESolution,
    beta: float,
    gamma: float,
    s_index: int = 0,
    N: Optional[float] = None,
) -> np.ndarray:
    """Effective reproduction number over time.

    ``Rt(t) = R₀ × S(t) / N``.

    Parameters
    ----------
    solution : ODESolution from a model run
    beta, gamma : model parameters
    s_index : index of the S compartment in the state vector
    N : total population (if None, inferred as sum of initial states)
    """
    S = solution.y[s_index]
    if N is None:
        N = solution.y[:, 0].sum()
    R0 = compute_R0(beta, gamma)
    return R0 * S / N


# ---------------------------------------------------------------------------
# Peak infection
# ---------------------------------------------------------------------------

def peak_infections(
    solution: ODESolution,
    i_index: int = 1,
) -> tuple[float, float]:
    """Return (peak_count, peak_time) for the infected compartment."""
    I = solution.y[i_index]
    idx = int(np.argmax(I))
    return float(I[idx]), float(solution.t[idx])


# ---------------------------------------------------------------------------
# Attack rate and final size
# ---------------------------------------------------------------------------

def attack_rate(
    solution: ODESolution,
    N: Optional[float] = None,
    s_index: int = 0,
) -> float:
    """Fraction of the population that was ever susceptible → infected.

    ``AR = 1 - S(final) / N``.
    """
    S_final = solution.y[s_index, -1]
    if N is None:
        N = solution.y[:, 0].sum()
    return 1.0 - S_final / N


def final_size(
    solution: ODESolution,
    r_index: int = -1,
) -> float:
    """Value of the R compartment at the final time step."""
    return float(solution.y[r_index, -1])


# ---------------------------------------------------------------------------
# Epidemic duration
# ---------------------------------------------------------------------------

def epidemic_duration(
    solution: ODESolution,
    i_index: int = 1,
    threshold: float = 1.0,
) -> Optional[float]:
    """Time at which I first drops below *threshold* after the peak.

    Returns ``None`` if I never drops below threshold.
    """
    I = solution.y[i_index]
    peak_idx = int(np.argmax(I))
    tail = I[peak_idx:]
    below = np.where(tail < threshold)[0]
    if len(below) == 0:
        return None
    return float(solution.t[peak_idx + below[0]])


# ---------------------------------------------------------------------------
# Aggregate helper
# ---------------------------------------------------------------------------

def compute_metrics(
    solution: ODESolution,
    beta: float,
    gamma: float,
    N: Optional[float] = None,
    s_index: int = 0,
    i_index: int = 1,
    r_index: int = -1,
) -> EpidemicMetrics:
    """Compute all summary metrics from a single model trajectory."""
    if N is None:
        N = solution.y[:, 0].sum()
    R0 = compute_R0(beta, gamma)
    peak_i, peak_t = peak_infections(solution, i_index)
    ar = attack_rate(solution, N, s_index)
    fs = final_size(solution, r_index)
    dur = epidemic_duration(solution, i_index)
    return EpidemicMetrics(
        R0=R0,
        peak_infected=peak_i,
        peak_time=peak_t,
        attack_rate=ar,
        final_size=fs,
        total_pop=N,
        epidemic_duration=dur,
    )
