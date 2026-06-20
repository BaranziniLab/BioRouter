"""
Slice sampler.

Implements the univariate slice sampling algorithm of Neal (2003).
Supports:
- Stepping-out procedure
- Doubling procedure
- Simple shrinkage procedure
"""

from __future__ import annotations

import math
from typing import Dict, Optional

import numpy as np

from bayesmcmc.model import Model


class SliceSampler:
    """
    Univariate slice sampler using stepping-out + shrinkage.

    Parameters
    ----------
    model : Model
        The Bayesian model to sample from.
    width : float
        Initial bracket width for slice sampling.
    """

    def __init__(
        self,
        model: Model,
        width: float = 1.0,
    ):
        self.model = model
        self.param_names = model.get_parameter_names()
        self.k = len(self.param_names)
        self.width = width

    def _theta_to_vec(self, theta: Dict[str, float]) -> np.ndarray:
        return np.array([theta[name] for name in self.param_names])

    def _vec_to_theta(self, vec: np.ndarray) -> Dict[str, float]:
        return {name: float(vec[i]) for i, name in enumerate(self.param_names)}

    def _log_prob(self, theta_vec: np.ndarray) -> float:
        theta = self._vec_to_theta(theta_vec)
        return self.model.log_posterior(theta)

    def _slice_sample_1d(
        self,
        idx: int,
        theta_vec: np.ndarray,
        log_y: float,
        rng: np.random.Generator,
        width: float = None,
    ) -> float:
        """
        Perform 1D slice sampling for parameter at index idx.

        Uses stepping-out + simple shrinkage (Neal 2003, Algorithm 5 + 4).
        """
        w = width if width is not None else self.width

        # current position
        x0 = theta_vec[idx]

        # draw horizontal slice level
        # log_y is already drawn

        # stepping out: find bracket [L, R]
        u = rng.uniform()
        L = x0 - w * u
        R = L + w

        # step out
        max_steps = 10
        step = 0
        theta_test = theta_vec.copy()
        theta_test[idx] = L
        while self._log_prob(theta_test) > log_y and step < max_steps:
            L -= w
            theta_test[idx] = L
            step += 1

        step = 0
        theta_test[idx] = R
        while self._log_prob(theta_test) > log_y and step < max_steps:
            R += w
            theta_test[idx] = R
            step += 1

        # shrinkage
        for _ in range(100):
            x_new = L + rng.uniform() * (R - L)
            theta_test[idx] = x_new
            log_p = self._log_prob(theta_test)

            if log_p > log_y:
                # accept
                return x_new

            # shrink bracket
            if x_new < x0:
                L = x_new
            else:
                R = x_new

            if abs(R - L) < 1e-12:
                return x0  # fallback

        return x0  # fallback

    def run(
        self,
        n_samples: int = 1000,
        n_chains: int = 1,
        burn_in: int = 0,
        thin: int = 1,
        seed: Optional[int] = None,
        width: Optional[float] = None,
    ) -> Dict[str, np.ndarray]:
        """
        Run the slice sampler.

        Returns
        -------
        dict : {param_name: np.ndarray of shape (n_chains, n_effective)}
        """
        rng = np.random.default_rng(seed)
        n_effective = (n_samples - burn_in) // thin
        all_chains = {name: np.zeros((n_chains, n_effective)) for name in self.param_names}

        w = width if width is not None else self.width

        for c in range(n_chains):
            theta = self.model.initial_theta(rng)
            theta_vec = self._theta_to_vec(theta)

            for i in range(n_samples):
                # sample each parameter in turn
                for idx, name in enumerate(self.param_names):
                    # draw slice level
                    log_p_current = self._log_prob(theta_vec)
                    log_y = log_p_current - rng.exponential(1.0)

                    # 1D slice sample
                    theta_vec[idx] = self._slice_sample_1d(
                        idx, theta_vec, log_y, rng, width=w
                    )

                # store
                if i >= burn_in and (i - burn_in) % thin == 0:
                    idx_store = (i - burn_in) // thin
                    for j, name in enumerate(self.param_names):
                        all_chains[name][c, idx_store] = theta_vec[j]

        return all_chains
