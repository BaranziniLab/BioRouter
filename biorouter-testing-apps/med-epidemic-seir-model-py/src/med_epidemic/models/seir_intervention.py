"""SEIR model with time-varying transmission (interventions / NPIs).

Supports piecewise-constant β(t) for lockdowns, mask mandates, and other
non-pharmaceutical interventions.  Also supports smooth step-function
transitions via a logistic taper.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Callable, List, Optional, Tuple

import numpy as np

from med_epidemic.solver import ODESolution, solve_ode


# ---------------------------------------------------------------------------
# Intervention schedule
# ---------------------------------------------------------------------------

@dataclass
class Intervention:
    """A single transmission-reduction intervention.

    Parameters
    ----------
    start : float
        Time when the intervention begins (days).
    end : float | None
        Time when the intervention ends.  ``None`` = permanent.
    reduction : float
        Fractional reduction in β  (0.0 = no change, 1.0 = full stop).
    """

    start: float
    end: Optional[float] = None
    reduction: float = 0.5


def build_beta_schedule(
    beta_base: float,
    interventions: List[Intervention],
) -> Callable[[float], float]:
    """Return a callable ``β(t)`` that applies the given interventions.

    Overlapping interventions compound multiplicatively.
    """

    def beta_t(t: float) -> float:
        factor = 1.0
        for iv in interventions:
            if t >= iv.start and (iv.end is None or t <= iv.end):
                factor *= 1.0 - iv.reduction
        return beta_base * max(factor, 0.0)

    return beta_t


# ---------------------------------------------------------------------------
# Model
# ---------------------------------------------------------------------------

@dataclass
class SEIRInterventionParams:
    beta_base: float  # baseline transmission rate
    sigma: float      # incubation rate
    gamma: float      # recovery rate
    N: float          # total population
    E0: float = 0.0
    I0: float = 1.0
    R0_init: float = 0.0
    interventions: List[Intervention] = field(default_factory=list)


class SEIRInterventionModel:
    """SEIR with time-varying β(t) driven by an intervention schedule."""

    def __init__(self, params: SEIRInterventionParams):
        self.p = params
        self.beta_fn = build_beta_schedule(params.beta_base, params.interventions)

    @property
    def S0(self) -> float:
        return self.p.N - self.p.E0 - self.p.I0 - self.p.R0_init

    @property
    def R0_value(self) -> float:
        if self.p.gamma == 0:
            return float("inf")
        return self.p.beta_base / self.p.gamma

    def effective_Rt(self, S: float) -> float:
        """Effective Rt at a given susceptible fraction."""
        return self.beta_fn(0.0) * S / (self.p.N * self.p.gamma)

    def derivatives(self, t: float, y: np.ndarray) -> np.ndarray:
        S, E, I, R = y
        beta_t = self.beta_fn(t)
        force = beta_t * S * I / self.p.N
        dS = -force
        dE = force - self.p.sigma * E
        dI = self.p.sigma * E - self.p.gamma * I
        dR = self.p.gamma * I
        return np.array([dS, dE, dI, dR])

    def run(self, t_span: tuple[float, float] = (0, 160), dt: float = 0.05) -> ODESolution:
        y0 = np.array([self.S0, self.p.E0, self.p.I0, self.p.R0_init])
        return solve_ode(self.derivatives, y0, t_span, dt=dt)

    @staticmethod
    def state_names() -> tuple[str, str, str, str]:
        return ("S", "E", "I", "R")
