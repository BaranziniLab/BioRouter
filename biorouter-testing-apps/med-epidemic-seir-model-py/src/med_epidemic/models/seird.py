"""SEIRD compartmental model (Susceptible → Exposed → Infected → Recovered / Dead).

Equations::

    dS/dt = -β * S * I / N
    dE/dt =  β * S * I / N  - σ * E
    dI/dt =  σ * E          - (γ + μ) * I
    dR/dt =  γ * I
    dD/dt =  μ * I

where μ = mortality rate (case-fatality rate per unit time).
"""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np

from med_epidemic.solver import ODESolution, solve_ode


@dataclass
class SEIRDParams:
    beta: float   # transmission rate
    sigma: float  # incubation rate
    gamma: float  # recovery rate
    mu: float     # mortality rate
    N: float      # total population
    E0: float = 0.0
    I0: float = 1.0
    R0: float = 0.0
    D0: float = 0.0


class SEIRDModel:
    """Deterministic SEIRD model."""

    def __init__(self, params: SEIRDParams):
        self.p = params
        self._validate()

    def _validate(self) -> None:
        p = self.p
        if p.N <= 0:
            raise ValueError("N must be > 0")
        if p.beta < 0 or p.sigma <= 0 or p.gamma <= 0 or p.mu < 0:
            raise ValueError("Invalid rates")
        if any(x < 0 for x in (p.I0, p.R0, p.E0, p.D0)):
            raise ValueError("compartments must be >= 0")
        if p.E0 + p.I0 + p.R0 + p.D0 > p.N:
            raise ValueError("initial compartments exceed N")

    @property
    def S0(self) -> float:
        return self.p.N - self.p.E0 - self.p.I0 - self.p.R0 - self.p.D0

    @property
    def R0_value(self) -> float:
        removal_rate = self.p.gamma + self.p.mu
        if removal_rate == 0:
            return float("inf")
        return self.p.beta / removal_rate

    def derivatives(self, t: float, y: np.ndarray) -> np.ndarray:
        S, E, I, R, D = y
        N = self.p.N
        force = self.p.beta * S * I / N
        dS = -force
        dE = force - self.p.sigma * E
        dI = self.p.sigma * E - (self.p.gamma + self.p.mu) * I
        dR = self.p.gamma * I
        dD = self.p.mu * I
        return np.array([dS, dE, dI, dR, dD])

    def run(self, t_span: tuple[float, float] = (0, 200), dt: float = 0.05) -> ODESolution:
        y0 = np.array([self.S0, self.p.E0, self.p.I0, self.p.R0, self.p.D0])
        return solve_ode(self.derivatives, y0, t_span, dt=dt)

    @staticmethod
    def state_names() -> tuple[str, str, str, str, str]:
        return ("S", "E", "I", "R", "D")
