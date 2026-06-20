"""Tests for the stochastic (Gillespie SSA) module.

Key test: for large N, the stochastic mean should approximate the
deterministic trajectory.
"""

import numpy as np
import pytest

from med_epidemic.stochastic import (
    run_sir_gillespie,
    run_seir_gillespie,
    run_seird_gillespie,
    run_ensemble,
    ensemble_mean,
)
from med_epidemic.models.sir import SIRModel, SIRParams


class TestGillespieSIR:
    def test_total_population_constant(self):
        """S + I + R == N at every event."""
        N = 100
        t, y = run_sir_gillespie(N=N, beta=0.5, gamma=0.2, I0=5,
                                  t_span=(0, 50), rng=np.random.default_rng(42))
        totals = y.sum(axis=0)
        assert np.all(totals == N)

    def test_compartments_nonneg(self):
        N = 500
        t, y = run_sir_gillespie(N=N, beta=0.3, gamma=0.1, I0=10,
                                  t_span=(0, 100), rng=np.random.default_rng(123))
        assert np.all(y >= 0)

    def test_infection_spreads_with_R0_gt_1(self):
        """With R0 > 1, some recovery should happen (R > 0 at end)."""
        N = 500
        beta, gamma = 0.5, 0.2  # R0 = 2.5
        t, y = run_sir_gillespie(N=N, beta=beta, gamma=gamma, I0=5,
                                  t_span=(0, 100), rng=np.random.default_rng(42))
        assert y[2, -1] > 0  # R > 0

    def test_no_spread_when_R0_below_1(self):
        """With R0 < 1, a single infected person should recover without causing many infections."""
        N = 500
        beta, gamma = 0.1, 0.5  # R0 = 0.2
        t, y = run_sir_gillespie(N=N, beta=beta, gamma=gamma, I0=1,
                                  t_span=(0, 100), rng=np.random.default_rng(42))
        # I should go to 0 and S should remain near N
        assert y[1, -1] == 0
        assert y[0, -1] >= N - 5  # at most a handful got infected


class TestGillespieSEIR:
    def test_total_population_constant(self):
        N = 200
        t, y = run_seir_gillespie(N=N, beta=0.5, sigma=0.2, gamma=0.1,
                                   I0=5, E0=2, t_span=(0, 50),
                                   rng=np.random.default_rng(42))
        assert np.all(y.sum(axis=0) == N)


class TestGillespieSEIRD:
    def test_total_population_constant(self):
        N = 200
        t, y = run_seird_gillespie(N=N, beta=0.5, sigma=0.2, gamma=0.1,
                                    mu=0.02, I0=5, E0=2, t_span=(0, 50),
                                    rng=np.random.default_rng(42))
        assert np.all(y.sum(axis=0) == N)


class TestEnsemble:
    def test_run_ensemble_count(self):
        results = run_ensemble(
            lambda rng, **kw: run_sir_gillespie(N=100, beta=0.3, gamma=0.1, I0=2,
                                                 t_span=(0, 20), rng=rng),
            n_runs=5,
            seed=42,
        )
        assert len(results) == 5

    def test_ensemble_mean_shape(self):
        results = run_ensemble(
            lambda rng, **kw: run_sir_gillespie(N=100, beta=0.3, gamma=0.1, I0=2,
                                                 t_span=(0, 30), rng=rng),
            n_runs=5,
            seed=42,
        )
        t_grid, y_mean = ensemble_mean(results, n_states=3, n_time_points=200)
        assert t_grid.shape == (200,)
        assert y_mean.shape == (3, 200)


class TestStochasticApproximatesDeterministic:
    """For large N, stochastic ensemble mean ≈ deterministic SIR."""

    def test_sir_stochastic逼近_deterministic(self):
        N = 50000
        beta, gamma = 0.3, 0.1
        I0 = 50

        # deterministic
        det_model = SIRModel(SIRParams(beta=beta, gamma=gamma, N=N, I0=I0))
        det_sol = det_model.run(t_span=(0, 100), dt=0.5)

        # stochastic ensemble (small number of runs for speed)
        results = run_ensemble(
            lambda rng, **kw: run_sir_gillespie(N=N, beta=beta, gamma=gamma,
                                                 I0=I0, t_span=(0, 100), rng=rng),
            n_runs=10,
            seed=42,
        )
        t_grid, y_mean = ensemble_mean(results, n_states=3, n_time_points=200)

        # compare I trajectories at sampled points
        det_I_interp = np.interp(t_grid, det_sol.t, det_sol.y[1])

        # Allow 15% relative tolerance (stochastic noise)
        peak_det = det_I_interp.max()
        peak_stoch = y_mean[1].max()
        assert abs(peak_stoch - peak_det) / peak_det < 0.15
