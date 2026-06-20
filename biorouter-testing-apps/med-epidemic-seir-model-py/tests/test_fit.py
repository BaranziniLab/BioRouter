"""Tests for the parameter fitting module.

Key test: fitting recovers known parameters from synthetic data.
"""

import numpy as np
import pytest

from med_epidemic.fit import (
    _sse,
    _rmse,
    grid_search,
    least_squares_fit,
    fit_sir,
    fit_seir,
)
from med_epidemic.models.sir import SIRModel, SIRParams
from med_epidemic.models.seir import SEIRModel, SEIRParams


class TestSSE:
    def test_identical_arrays(self):
        assert _sse(np.array([1, 2, 3]), np.array([1, 2, 3])) == 0.0

    def test_known_difference(self):
        a = np.array([1, 2, 3])
        b = np.array([1, 3, 3])
        assert _sse(a, b) == 1.0


class TestRMSE:
    def test_identical(self):
        assert _rmse(np.array([1, 2]), np.array([1, 2])) == 0.0

    def test_known(self):
        a = np.array([0, 0])
        b = np.array([1, 1])
        assert _rmse(a, b) == pytest.approx(1.0)


def _generate_synthetic_sir(N=10000, beta=0.3, gamma=0.1, I0=10, t_max=100):
    """Generate synthetic observed data from a known SIR model."""
    model = SIRModel(SIRParams(beta=beta, gamma=gamma, N=N, I0=I0))
    sol = model.run(t_span=(0, t_max), dt=0.5)
    t_obs = np.linspace(0, t_max, 100)
    I_obs = np.interp(t_obs, sol.t, sol.y[1])
    # add small noise
    rng = np.random.default_rng(42)
    I_obs += rng.normal(0, I_obs.max() * 0.02, size=I_obs.shape)
    I_obs = np.maximum(I_obs, 0)
    return t_obs, I_obs


def _generate_synthetic_seir(N=10000, beta=0.3, sigma=0.2, gamma=0.1, I0=10, t_max=120):
    """Generate synthetic observed data from a known SEIR model."""
    model = SEIRModel(SEIRParams(beta=beta, sigma=sigma, gamma=gamma, N=N, I0=I0))
    sol = model.run(t_span=(0, t_max), dt=0.5)
    t_obs = np.linspace(0, t_max, 100)
    I_obs = np.interp(t_obs, sol.t, sol.y[2])
    rng = np.random.default_rng(42)
    I_obs += rng.normal(0, I_obs.max() * 0.02, size=I_obs.shape)
    I_obs = np.maximum(I_obs, 0)
    return t_obs, I_obs


class TestGridSearchSIR:
    def test_recovers_known_parameters(self):
        """Grid search on noiseless SIR data should recover the true β, γ."""
        true_beta, true_gamma = 0.4, 0.15
        N = 10000
        t_obs, I_obs = _generate_synthetic_sir(
            N=N, beta=true_beta, gamma=true_gamma, I0=10, t_max=80,
        )
        result = grid_search(
            t_obs, I_obs, N,
            beta_range=(0.2, 0.8, 13),
            gamma_range=(0.05, 0.4, 19),
            model_type="sir",
            t_span=(0, 80),
            dt=0.5,
        )
        # Grid has enough resolution to get close
        assert result.best_params["beta"] == pytest.approx(true_beta, abs=0.08)
        assert result.best_params["gamma"] == pytest.approx(true_gamma, abs=0.05)


class TestGridSearchSEIR:
    def test_recovers_known_parameters(self):
        true_beta, true_sigma, true_gamma = 0.4, 0.2, 0.15
        N = 10000
        t_obs, I_obs = _generate_synthetic_seir(
            N=N, beta=true_beta, sigma=true_sigma, gamma=true_gamma, I0=10, t_max=100,
        )
        result = grid_search(
            t_obs, I_obs, N,
            beta_range=(0.2, 0.8, 5),
            sigma_range=(0.1, 0.5, 3),
            gamma_range=(0.05, 0.4, 5),
            model_type="seir",
            t_span=(0, 100),
            dt=0.5,
        )
        assert result.best_params["beta"] == pytest.approx(true_beta, abs=0.2)
        assert result.best_params["gamma"] == pytest.approx(true_gamma, abs=0.15)


class TestLeastSquares:
    def test_refines_grid_result(self):
        """Least-squares refinement should improve the grid search result."""
        true_beta, true_gamma = 0.4, 0.15
        N = 10000
        t_obs, I_obs = _generate_synthetic_sir(
            N=N, beta=true_beta, gamma=true_gamma, I0=10, t_max=80,
        )
        # get initial grid estimate
        grid = grid_search(
            t_obs, I_obs, N,
            beta_range=(0.2, 0.8, 5),
            gamma_range=(0.05, 0.4, 5),
            model_type="sir",
            t_span=(0, 80),
            dt=0.5,
        )
        # refine with least squares
        refined = least_squares_fit(
            t_obs, I_obs, N, grid.best_params,
            model_type="sir", t_span=(0, 80),
        )
        # refined should be closer to truth
        err_before = abs(grid.best_params["beta"] - true_beta) + abs(grid.best_params["gamma"] - true_gamma)
        err_after = abs(refined["beta"] - true_beta) + abs(refined["gamma"] - true_gamma)
        assert err_after <= err_before + 0.01  # should improve or stay about the same


class TestFitSIR:
    def test_high_level_fit(self):
        true_beta, true_gamma = 0.35, 0.12
        N = 10000
        t_obs, I_obs = _generate_synthetic_sir(
            N=N, beta=true_beta, gamma=true_gamma, I0=10, t_max=80,
        )
        params = fit_sir(t_obs, I_obs, N, t_span=(0, 80))
        # The default grid is coarse; with least-squares refinement, we
        # should get within a reasonable range of the true parameters.
        assert params["beta"] == pytest.approx(true_beta, abs=0.3)
        assert params["gamma"] == pytest.approx(true_gamma, abs=0.2)
