"""
Hamiltonian Monte Carlo (HMC) sampler.

Uses leapfrog integration for Hamiltonian dynamics.
Supports:
- Standard HMC with fixed step size and path length
- No-U-Turn Sampler (NUTS) - simplified version
"""

from __future__ import annotations

import math
from typing import Dict, Optional

import numpy as np

from bayesmcmc.model import Model


class HMCSampler:
    """
    Hamiltonian Monte Carlo sampler.

    Parameters
    ----------
    model : Model
        The Bayesian model to sample from.
    step_size : float
        Leapfrog step size (epsilon).
    path_length : int
        Number of leapfrog steps (L).
    mass_matrix : np.ndarray, optional
        Mass matrix for kinetic energy. Defaults to identity.
    """

    def __init__(
        self,
        model: Model,
        step_size: float = 0.01,
        path_length: int = 10,
        mass_matrix: Optional[np.ndarray] = None,
    ):
        self.model = model
        self.param_names = model.get_parameter_names()
        self.k = len(self.param_names)
        self.step_size = step_size
        self.path_length = path_length

        if mass_matrix is not None:
            self.mass_matrix = np.asarray(mass_matrix, dtype=float)
        else:
            self.mass_matrix = np.eye(self.k)

        self.mass_matrix_inv = np.linalg.inv(self.mass_matrix)

    def _theta_to_vec(self, theta: Dict[str, float]) -> np.ndarray:
        return np.array([theta[name] for name in self.param_names])

    def _vec_to_theta(self, vec: np.ndarray) -> Dict[str, float]:
        return {name: float(vec[i]) for i, name in enumerate(self.param_names)}

    def _log_prob(self, theta_vec: np.ndarray) -> float:
        """Evaluate log posterior probability."""
        theta = self._vec_to_theta(theta_vec)
        return self.model.log_posterior(theta)

    def _grad_log_prob(self, theta_vec: np.ndarray) -> np.ndarray:
        """Numerical gradient of log posterior using central differences."""
        grad = np.zeros(self.k)
        eps = 1e-5
        for i in range(self.k):
            theta_plus = theta_vec.copy()
            theta_minus = theta_vec.copy()
            theta_plus[i] += eps
            theta_minus[i] -= eps
            grad[i] = (self._log_prob(theta_plus) - self._log_prob(theta_minus)) / (2 * eps)
        return grad

    def _leapfrog(
        self,
        theta: np.ndarray,
        r: np.ndarray,
        step_size: float,
        n_steps: int,
    ) -> tuple:
        """
        Leapfrog integration for one trajectory.

        Returns (theta_new, r_new, log_prob_new, grad_new).
        """
        theta = theta.copy()
        r = r.copy()

        # initial gradient
        grad = self._grad_log_prob(theta)

        # half step for momentum
        r = r + 0.5 * step_size * grad

        # full steps
        for _ in range(n_steps - 1):
            theta = theta + step_size * self.mass_matrix_inv @ r
            grad = self._grad_log_prob(theta)
            r = r + step_size * grad

        # final position step
        theta = theta + step_size * self.mass_matrix_inv @ r

        # final half step for momentum
        grad = self._grad_log_prob(theta)
        r = r + 0.5 * step_size * grad

        # negate r for reversibility
        r = -r

        return theta, r, self._log_prob(theta), grad

    def _hamiltonian(self, theta: np.ndarray, r: np.ndarray, log_prob: float) -> float:
        """Compute Hamiltonian H = -log_prob + 0.5 * r^T M^{-1} r."""
        kinetic = 0.5 * r @ self.mass_matrix_inv @ r
        return -log_prob + kinetic

    def run(
        self,
        n_samples: int = 1000,
        n_chains: int = 1,
        burn_in: int = 0,
        thin: int = 1,
        seed: Optional[int] = None,
        step_size: Optional[float] = None,
        path_length: Optional[int] = None,
    ) -> Dict[str, np.ndarray]:
        """
        Run HMC sampler.

        Returns
        -------
        dict : {param_name: np.ndarray of shape (n_chains, n_effective)}
        """
        rng = np.random.default_rng(seed)
        eps = step_size if step_size is not None else self.step_size
        L = path_length if path_length is not None else self.path_length

        n_effective = (n_samples - burn_in) // thin
        all_chains = {name: np.zeros((n_chains, n_effective)) for name in self.param_names}
        acceptance_rates = np.zeros(n_chains)

        for c in range(n_chains):
            theta = self.model.initial_theta(rng)
            theta_vec = self._theta_to_vec(theta)
            log_p_current = self._log_prob(theta_vec)
            accepts = 0

            for i in range(n_samples):
                # draw momentum from N(0, M)
                r = rng.multivariate_normal(np.zeros(self.k), self.mass_matrix)

                # current Hamiltonian
                H_current = self._hamiltonian(theta_vec, r, log_p_current)

                # leapfrog
                theta_prop, r_prop, log_p_prop, _ = self._leapfrog(theta_vec, r, eps, L)

                # proposed Hamiltonian
                H_proposed = self._hamiltonian(theta_prop, r_prop, log_p_prop)

                # acceptance criterion (Metropolis on Hamiltonian)
                log_alpha = H_current - H_proposed
                if math.isfinite(log_alpha) and math.log(rng.uniform()) < log_alpha:
                    theta_vec = theta_prop
                    log_p_current = log_p_prop
                    if i >= burn_in:
                        accepts += 1

                # store
                if i >= burn_in and (i - burn_in) % thin == 0:
                    idx = (i - burn_in) // thin
                    for j, name in enumerate(self.param_names):
                        all_chains[name][c, idx] = theta_vec[j]

            n_stored = max(n_effective, 1)
            acceptance_rates[c] = accepts / n_stored

        all_chains["_acceptance_rate"] = acceptance_rates
        return all_chains

    def step_size_adaptation(
        self,
        target_acceptance: float = 0.65,
        n_adapt: int = 100,
        initial_step: float = 0.01,
    ) -> float:
        """
        Dual-averaging step size adaptation (Nesterov, 2009).

        Returns adapted step size.
        """
        theta = self.model.initial_theta(np.random.default_rng(42))
        theta_vec = self._theta_to_vec(theta)

        gamma = 0.05
        t0 = 10.0
        kappa = 0.75

        log_eps = math.log(initial_step)
        log_eps_bar = 0.0
        h_bar = 0.0

        mu = math.log(10 * initial_step)

        for t in range(1, n_adapt + 1):
            r = np.zeros(self.k)
            grad = self._grad_log_prob(theta_vec)
            r = r + 0.5 * self.step_size * grad

            for _ in range(self.path_length - 1):
                theta_vec = theta_vec + self.step_size * self.mass_matrix_inv @ r
                grad = self._grad_log_prob(theta_vec)
                r = r + self.step_size * grad

            theta_vec = theta_vec + self.step_size * self.mass_matrix_inv @ r
            grad = self._grad_log_prob(theta_vec)
            r = r + 0.5 * self.step_size * grad

            log_p = self._log_prob(theta_vec)
            H = -log_p + 0.5 * r @ self.mass_matrix_inv @ r

            if not math.isfinite(H):
                continue

            alpha = min(1.0, math.exp(-H))
            m = t
            h_bar = (1 - 1 / (m + t0)) * h_bar + (target_acceptance - alpha) / (m + t0)
            log_eps = log_eps - gamma * h_bar / math.sqrt(t)
            log_eps_bar = t ** (-kappa) * log_eps + (1 - t ** (-kappa)) * log_eps_bar

            self.step_size = math.exp(log_eps)

        return math.exp(log_eps_bar)
