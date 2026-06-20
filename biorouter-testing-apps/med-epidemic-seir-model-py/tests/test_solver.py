"""Tests for the RK4 ODE solver."""

import numpy as np
import pytest

from med_epidemic.solver import ODESolution, rk4_step, solve_ode, Event


class TestRK4Step:
    """Test the single-step RK4 function."""

    def test_decay_analytic(self):
        """dy/dt = -y  →  y(t) = y0 * exp(-t)."""
        f = lambda t, y: -y
        y = np.array([10.0])
        dt = 0.01
        # 100 steps = t=1.0
        for _ in range(100):
            y = rk4_step(f, 0, y, dt)
        expected = 10.0 * np.exp(-1.0)
        assert abs(y[0] - expected) < 1e-6

    def test_constant_derivative(self):
        """dy/dt = 2  →  y(t) = y0 + 2t."""
        f = lambda t, y: np.array([2.0])
        y = np.array([0.0])
        y = rk4_step(f, 0, y, 1.0)
        assert abs(y[0] - 2.0) < 1e-12

    def test_coupled_system(self):
        """Two-dimensional harmonic oscillator: dx/dt=y, dy/dt=-x
        Solution: x=sin(t), y=cos(t) at small dt.
        """
        f = lambda t, y: np.array([y[1], -y[0]])
        y0 = np.array([0.0, 1.0])
        dt = 0.001
        y = y0.copy()
        t = 0.0
        for _ in range(int(np.pi / 2 / dt)):
            y = rk4_step(f, t, y, dt)
            t += dt
        # at t = pi/2, x ~ 1, y ~ 0
        assert abs(y[0] - 1.0) < 0.001
        assert abs(y[1]) < 0.001


class TestSolveODE:
    """Test the full ODE integrator."""

    def test_decay_over_interval(self):
        """Integrate dy/dt = -2y from t=0..5."""
        f = lambda t, y: -2.0 * y
        sol = solve_ode(f, np.array([1.0]), (0, 5), dt=0.01)
        expected = np.exp(-10.0)
        assert abs(sol.y[0, -1] - expected) < 1e-4

    def test_solution_shape(self):
        f = lambda t, y: np.array([-y[0], y[0]])
        sol = solve_ode(f, np.array([1.0, 0.0]), (0, 10), dt=0.1)
        assert sol.y.shape == (2, sol.t.shape[0])
        assert sol.n_states == 2
        assert sol.n_steps == sol.t.shape[0]

    def test_getitem(self):
        f = lambda t, y: np.array([-y[0], y[0]])
        sol = solve_ode(f, np.array([1.0, 0.0]), (0, 1), dt=0.1)
        assert np.allclose(sol[0], sol.y[0])
        assert np.allclose(sol[1], sol.y[1])

    def test_linear_ode_accuracy(self):
        """dy/dt = 0.2y → y(t) = y0 * exp(0.2t)."""
        f = lambda t, y: 0.2 * y
        sol = solve_ode(f, np.array([1.0]), (0, 10), dt=0.1)
        expected = np.exp(2.0)
        assert abs(sol.y[0, -1] - expected) / expected < 1e-6

    def test_events_stop_integration(self):
        """Event that stops when y drops below 1."""
        f = lambda t, y: -0.5 * y
        ev = Event(callback=lambda t, y: y[0] - 1.0, terminal=True)
        sol = solve_ode(f, np.array([10.0]), (0, 100), dt=0.1, events=[ev])
        # should stop before t=100
        assert sol.t[-1] < 100
        assert sol.y[0, -1] < 2.0  # near threshold
