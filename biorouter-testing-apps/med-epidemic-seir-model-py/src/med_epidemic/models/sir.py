"""SIR compartmental model (Susceptible → Infected → Recovered).

Equations::

    dS/dt = -β * S * I / N
    dI/dt =  β * S * I / N  - γ * I
    dR/dt =  γ * I

where β = transmission rate, γ = recovery rate, N = total population.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional

import numpy as np

from med_epidemic.solver import ODESolution, solve_ode


@dataclass
class SIRParams:
    beta: float   # transmission rate
    gamma: float  # recovery rate
    N: float      # total population
    I0: float = 1.0   # initial infected
    R0: float = 0.0   # initial recovered


class SIRModel:
    """Deterministic SIR model solved with RK4."""

    def __init__(self, params: SIRParams):
        self.p = params
        self._validate()

    def _validate(self) -> None:
        p = self.p
        if p.N <= 0:
            raise ValueError("N must be > 0")
        if p.beta < 0 or p.gamma < 0:
            raise ValueError("beta and gamma must be >= 0")
        if p.I0 < 0 or p.R0 < 0:
            raise ValueError("initial compartments must be >= 0")
        if p.I0 + p.R0 > p.N:
            raise ValueError("I0 + R0 must be <= N")

    @property
    def S0(self) -> float:
        return self.p.N - self.p.I0 - self.p.R0

    @property
    def R0_value(self) -> float:
        """Basic reproduction number R₀ = β/γ."""
        if self.p.gamma == 0:
            return float("inf")
        return self.p.beta / self.p.gamma

    def derivatives(self, t: float, y: np.ndarray) -> np.ndarray:
        S, I, R = y
        N = self.p.N
        dS = -self.p.beta * S * I / N
        dI = self.p.beta * S * I / N - self.p.gamma * I
        dR = self.p.gamma * I
        return np.array([dS, dI, dR])

    def run(self, t_span: tuple[float, float] = (0, 100), dt: float = 0.05) -> ODESolution:
        y0 = np.array([self.S0, self.p.I0, self.p.R0])
        return solve_ode(self.derivatives, y0, t_span, dt=dt)

    @staticmethod
    def state_names() -> tuple[str, str, str]:
        return ("S", "I", "R")


def sir_analytic_final_size(R0: float) -> float:
    """Solve the SIR transcendental final-size equation.

    ``r = 1 - exp(-R0 * r)`` where *r* is the attack rate (fraction infected).

    Uses Newton-Raphson iteration.
    """
    if R0 <= 0:
        return 0.0
    r = 1 - 1e-6  # initial guess near 1
    for _ in range(200):
        f = 1 - np.exp(-R0 * r) - r
        fp = R0 * np.exp(-R0 * r) - 1
        r_new = r - f / fp
        if abs(r_new - r) < 1e-12:
            break
        r = r_new
    return max(r, 0.0)
