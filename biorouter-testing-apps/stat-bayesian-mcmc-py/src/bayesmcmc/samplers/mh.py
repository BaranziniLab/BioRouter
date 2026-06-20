"""
Metropolis-Hastings sampler.

Supports:
- Random-walk MH with Gaussian proposals
- Adaptive proposal covariance (shrinking to posterior)
- Tuneable step-size
"""

from __future__ import annotations

import math
from typing import Dict, List, Optional, Tuple

import numpy as np

from bayesmcmc.model import Model


class MetropolisHastings:
    """
    Metropolis-Hastings sampler with (optionally adaptive) random-walk Gaussian proposals.

    Parameters
    ----------
    model : Model
        The Bayesian model to sample from.
    step_sizes : dict, optional
        Per-parameter proposal standard deviations. If None, set to 0.1.
    """

    def __init__(
        self,
        model: Model,
        step_sizes: Optional[Dict[str, float]] = None,
    ):
        self.model = model
        self.param_names = model.get_parameter_names()
        self.k = len(self.param_names)

        # default step sizes
        if step_sizes is not None:
            self.step_sizes = np.array([step_sizes[name] for name in self.param_names])
        else:
            self.step_sizes = np.full(self.k, 0.1)

        # adaptive proposal state
        self._proposal_cov = np.diag(self.step_sizes ** 2)
        self._proposal_cov_inv = np.diag(1.0 / (self.step_sizes ** 2 + 1e-300))
        self._sample_mean = np.zeros(self.k)
        self._sample_m2 = np.zeros((self.k, self.k))  # for Welford's online variance
        self._sample_count = 0

    def _theta_to_vec(self, theta: Dict[str, float]) -> np.ndarray:
        return np.array([theta[name] for name in self.param_names])

    def _vec_to_theta(self, vec: np.ndarray) -> Dict[str, float]:
        return {name: float(vec[i]) for i, name in enumerate(self.param_names)}

    def _proposal(self, theta_vec: np.ndarray, rng: np.random.Generator) -> np.ndarray:
        """Draw proposal from N(theta, proposal_cov)."""
        return rng.multivariate_normal(theta_vec, self._proposal_cov)

    def _proposal_log_ratio(
        self,
        theta_old: np.ndarray,
        theta_new: np.ndarray,
    ) -> float:
        """Log of proposal ratio q(theta_old|theta_new) / q(theta_new|theta_old).
        For symmetric random walk this is 0, but we keep the interface for
        potential non-symmetric proposals."""
        # Symmetric proposal: ratio = 0
        return 0.0

    def _adapt_proposal(self, theta_vec: np.ndarray, iteration: int, adapt_until: int = 500):
        """Welford's online adaptation of proposal covariance."""
        if iteration >= adapt_until:
            return
        self._sample_count += 1
        n = self._sample_count
        delta = theta_vec - self._sample_mean
        self._sample_mean += delta / n
        delta2 = theta_vec - self._sample_mean
        # outer product: ensure 2D result even for k=1
        outer = np.outer(delta, delta2)
        self._sample_m2 += outer

        if n >= 2:
            # sample covariance scaled by 2.38^2 / k (Gelman et al.)
            scale = (2.38 ** 2) / self.k
            sample_cov = self._sample_m2 / (n - 1)
            # Add diagonal loading to ensure positive-definiteness
            try:
                min_eig = np.min(np.linalg.eigvalsh(sample_cov))
            except np.linalg.LinAlgError:
                min_eig = 0.0
            diag_load = max(0, 1e-6 - min_eig) + 1e-6
            self._proposal_cov = scale * (sample_cov + diag_load * np.eye(self.k))
            try:
                self._proposal_cov_inv = np.linalg.inv(self._proposal_cov)
            except np.linalg.LinAlgError:
                pass

    def run(
        self,
        n_samples: int = 1000,
        n_chains: int = 1,
        burn_in: int = 0,
        thin: int = 1,
        seed: Optional[int] = None,
        adapt: bool = True,
        adapt_until: int = 500,
    ) -> Dict[str, np.ndarray]:
        """
        Run the sampler.

        Returns
        -------
        dict : {param_name: np.ndarray of shape (n_chains, n_effective)}
        """
        rng = np.random.default_rng(seed)
        n_effective = (n_samples - burn_in) // thin
        all_chains = {name: np.zeros((n_chains, n_effective)) for name in self.param_names}
        acceptance_rates = np.zeros(n_chains)

        for c in range(n_chains):
            # initialize
            if burn_in > 0:
                theta = self.model.initial_theta(rng)
            else:
                theta = self.model.initial_theta(rng)

            theta_vec = self._theta_to_vec(theta)
            log_p_current = self.model.log_posterior(theta)
            accepts = 0

            # reset proposal adaptation per chain
            if adapt:
                self._proposal_cov = np.diag(self.step_sizes ** 2)
                self._sample_mean = np.zeros(self.k)
                self._sample_m2 = np.zeros((self.k, self.k))
                self._sample_count = 0

            for i in range(n_samples):
                # propose
                theta_prop_vec = self._proposal(theta_vec, rng)
                theta_prop = self._vec_to_theta(theta_prop_vec)
                log_p_prop = self.model.log_posterior(theta_prop)

                # MH acceptance
                log_alpha = log_p_prop - log_p_current + self._proposal_log_ratio(theta_vec, theta_prop_vec)
                log_u = math.log(rng.uniform())

                if log_u < log_alpha:
                    theta_vec = theta_prop_vec
                    theta = theta_prop
                    log_p_current = log_p_prop
                    if i >= burn_in:
                        accepts += 1

                # adapt
                if adapt and i < adapt_until:
                    self._adapt_proposal(theta_vec, i, adapt_until)

                # store (after burn-in, with thinning)
                if i >= burn_in and (i - burn_in) % thin == 0:
                    idx = (i - burn_in) // thin
                    for name in self.param_names:
                        j = self.param_names.index(name)
                        all_chains[name][c, idx] = theta_vec[j]

            n_stored = n_effective
            acceptance_rates[c] = accepts / max(n_stored, 1)

        all_chains["_acceptance_rate"] = acceptance_rates
        return all_chains
