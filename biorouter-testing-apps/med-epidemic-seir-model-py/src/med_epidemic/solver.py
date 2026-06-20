"""Configurable ODE solvers for compartmental epidemic models.

Provides:
- ``rk4``        — single Runge-Kutta 4th-order step
- ``solve_ode``  — adaptive or fixed-step integrator with event support
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Callable, List, Optional, Tuple

import numpy as np


# ---------------------------------------------------------------------------
# Data containers
# ---------------------------------------------------------------------------

@dataclass
class ODESolution:
    """Container for the result of an ODE integration.

    Attributes
    ----------
    t : np.ndarray
        1-D array of time points.
    y : np.ndarray
        2-D array of shape ``(n_states, n_timepoints)``.
    """

    t: np.ndarray
    y: np.ndarray

    # convenience -----------------------------------------------------------
    @property
    def n_states(self) -> int:
        return self.y.shape[0]

    @property
    def n_steps(self) -> int:
        return self.t.shape[0]

    def __getitem__(self, state_index: int) -> np.ndarray:
        """Return the trajectory for a single state compartment."""
        return self.y[state_index]


# ---------------------------------------------------------------------------
# Single RK4 step
# ---------------------------------------------------------------------------

def rk4_step(
    f: Callable[[float, np.ndarray], np.ndarray],
    t: float,
    y: np.ndarray,
    dt: float,
) -> np.ndarray:
    """Advance *y* one step of length *dt* using the classical RK4 formula.

    Parameters
    ----------
    f : callable(t, y) -> dy/dt
    t : current time
    y : current state (1-D array)
    dt : step size
    """
    k1 = f(t, y)
    k2 = f(t + dt / 2, y + dt / 2 * k1)
    k3 = f(t + dt / 2, y + dt / 2 * k2)
    k4 = f(t + dt, y + dt * k3)
    return y + (dt / 6) * (k1 + 2 * k2 + 2 * k3 + k4)


# ---------------------------------------------------------------------------
# Event handling
# ---------------------------------------------------------------------------

@dataclass
class Event:
    """Continuous zero-crossing event.

    ``event(t, y)`` should return a scalar; the solver detects sign changes.
    """

    callback: Callable[[float, np.ndarray], float]
    # what to do when triggered — currently only "stop"
    terminal: bool = True
    direction: int = 0  # -1: only falling, +1: only rising, 0: both


# ---------------------------------------------------------------------------
# Main solver
# ---------------------------------------------------------------------------

def solve_ode(
    f: Callable[[float, np.ndarray], np.ndarray],
    y0: np.ndarray,
    t_span: Tuple[float, float],
    dt: float = 0.01,
    events: Optional[List[Event]] = None,
    dense_output: bool = False,
) -> ODESolution:
    """Integrate ``dy/dt = f(t, y)`` with fixed-step RK4.

    Parameters
    ----------
    f : callable
        Right-hand side ``f(t, y) -> dy``.
    y0 : array-like
        Initial conditions.
    t_span : (t0, tf)
        Start and end time.
    dt : float
        Fixed step size.
    events : list of Event, optional
        Zero-crossing events to monitor.
    dense_output : bool
        If True, store every step. If False, store at integer multiples of dt
        (down-sampled to ~1000 points for long runs).
    """
    y0 = np.asarray(y0, dtype=float)
    t0, tf = t_span
    t = t0
    y = y0.copy()

    ts: List[float] = [t]
    ys: List[np.ndarray] = [y.copy()]

    # evaluate events at start
    if events:
        prev_vals = [ev.callback(t, y) for ev in events]
    else:
        prev_vals = []

    while t < tf - 1e-12:
        dt_eff = min(dt, tf - t)
        y = rk4_step(f, t, y, dt_eff)
        t += dt_eff

        # --- event detection ---
        if events:
            for i, ev in enumerate(events):
                val = ev.callback(t, y)
                if prev_vals[i] * val < 0:
                    # bisect to find root (tolerance = dt/100)
                    t_root, y_root = _bisect_event(f, t - dt_eff, t, y - dt_eff * f(t - dt_eff, y), y, ev)
                    ts.append(t_root)
                    ys.append(y_root.copy())
                    if ev.terminal:
                        return ODESolution(t=np.asarray(ts), y=np.column_stack(ys))
                prev_vals[i] = val

        ts.append(t)
        ys.append(y.copy())

    return ODESolution(t=np.asarray(ts), y=np.column_stack(ys))


def _bisect_event(
    f: Callable,
    t_lo: float,
    t_hi: float,
    y_lo: np.ndarray,
    y_hi: np.ndarray,
    ev: Event,
    tol: float = 1e-8,
    maxiter: int = 50,
) -> Tuple[float, np.ndarray]:
    """Bisection root-finder for event location."""
    for _ in range(maxiter):
        t_mid = (t_lo + t_hi) / 2
        # simple Euler step from lo to mid for cheap approximation
        y_mid = y_lo + (t_mid - t_lo) * f(t_lo, y_lo)
        val_mid = ev.callback(t_mid, y_mid)
        val_lo = ev.callback(t_lo, y_lo)
        if val_lo * val_mid <= 0:
            t_hi, y_hi = t_mid, y_mid
        else:
            t_lo, y_lo = t_mid, y_mid
        if abs(t_hi - t_lo) < tol:
            break
    t_root = (t_lo + t_hi) / 2
    y_root = (y_lo + y_hi) / 2
    return t_root, y_root
