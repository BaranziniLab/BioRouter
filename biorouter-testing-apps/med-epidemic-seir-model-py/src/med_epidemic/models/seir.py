"""SEIR compartmental model (Susceptible → Exposed → Infected → Recovered).

Equations::

    dS/dt = -β * S * I / N
    dE/dt =  β * S * I / N  - σ * E
    dI/dt =  σ * E          - γ * I
    dR/dt =  γ * I

where σ = incubation rate (1/σ = mean latent period).
"""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np

from med_epidemic.solver import ODESolution, solve_ode


@dataclass
class SEIRParams:
    beta: float   # transmission rate
    sigma: float  # incubation rate (1/latent period)
    gamma: float  # recovery rate
    N: float      # total population
    E0: float = 0.0
    I0: float = 1.0
    R0: float = 0.0


class SEIRModel:
    """Deterministic SEIR model."""

    def __init__(self, params: SEIRParams):
        self.p = params
        self._validate()

    def _validate(self) -> None:
        p = self.p
        if p.N <= 0:
            raise ValueError("N must be > 0")
        if p.beta < 0 or p.sigma <= 0 or p.gamma <= 0:
            raise ValueError("beta >= 0; sigma, gamma > 0")
        if p.I0 < 0 or p.R0 < 0 or p.E0 < 0:
            raise ValueError("compartments must be >= 0")
        if p.E0 + p.I0 + p.R0 > p.N:
            raise ValueError("E0+I0+R0 must be <= N")

    @property
    def S0(self) -> float:
        return self.p.N - self.p.E0 - self.p.I0 - self.p.R0

    @property
    def R0_value(self) -> float:
        if self.p.gamma == 0:
            return float("inf")
        return self.p.beta / self.p.gamma

    def derivatives(self, t: float, y: np.ndarray) -> np.ndarray:
        S, E, I, R = y
        N = self.p.N
        infection_force = self.p.beta * S * I / N
        dS = -infection_force
        dE = infection_force - self.p.sigma * E
        dI = self.p.sigma * E - self.p.gamma * I
        dR = self.p.gamma * I
        return np.array([dS, dE, dI, dR])

    def run(self, t_span: tuple[float, float] = (0, 160), dt: float = 0.05) -> ODESolution:
        y0 = np.array([self.S0, self.p.E0, self.p.I0, self.p.R0])
        return solve_ode(self.derivatives, y0, t_span, dt=dt)

    @staticmethod
    def state_names() -> tuple[str, str, str, str]:
        return ("S", "E", "I", "R")
