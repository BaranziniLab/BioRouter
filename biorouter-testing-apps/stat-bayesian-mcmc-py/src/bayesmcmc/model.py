"""
Model specification API for Bayesian inference.

A Model encapsulates:
- Parameters with prior distributions
- A likelihood function
- Data
- Optional deterministic transformations
"""

from __future__ import annotations

import math
from typing import Any, Callable, Dict, List, Optional, Tuple, Union

import numpy as np

from bayesmcmc.distributions import Distribution, Normal, Beta, Gamma


class Parameter:
    """Represents a single model parameter with its prior."""

    def __init__(
        self,
        name: str,
        prior: Distribution,
        initial_value: Optional[float] = None,
        fixed: bool = False,
    ):
        self.name = name
        self.prior = prior
        self.fixed = fixed
        self.initial_value = initial_value if initial_value is not None else prior.sample(1, np.random.default_rng())[0]

    def log_prior(self, value: float) -> float:
        if self.fixed:
            if abs(value - self.initial_value) < 1e-12:
                return 0.0
            return -math.inf
        return self.log_pdf(value)

    def log_pdf(self, x: float) -> float:
        return self.prior.log_pdf(x)

    def sample_from_prior(self, rng: np.random.Generator) -> float:
        if self.fixed:
            return self.initial_value
        return float(self.prior.sample(1, rng)[0])

    def __repr__(self) -> str:
        return f"Parameter({self.name}, prior={self.prior.name})"


class Model:
    """
    Bayesian model specification.

    Usage:
        model = Model()
        model.add_parameter("mu", Normal(0, 10))
        model.add_parameter("sigma", Gamma(2, 2))
        model.set_likelihood(my_likelihood_fn)
        model.set_data(y_data)
    """

    def __init__(self, name: str = "unnamed"):
        self.name = name
        self.parameters: Dict[str, Parameter] = {}
        self._param_order: List[str] = []
        self._log_likelihood_fn: Optional[Callable[..., float]] = None
        self._data: Any = None
        self._deterministic: Dict[str, Callable] = {}

    # ----- parameter management -----

    def add_parameter(
        self,
        name: str,
        prior: Distribution,
        initial_value: Optional[float] = None,
        fixed: bool = False,
    ) -> "Model":
        self.parameters[name] = Parameter(name, prior, initial_value, fixed)
        if name not in self._param_order:
            self._param_order.append(name)
        return self

    def get_parameter_names(self) -> List[str]:
        return list(self._param_order)

    def get_parameter_values(self, theta: Dict[str, float]) -> np.ndarray:
        return np.array([theta[name] for name in self._param_order])

    # ----- likelihood -----

    def set_likelihood(self, fn: Callable[..., float]) -> "Model":
        """Set the log-likelihood function: fn(data, **params) -> float."""
        self._log_likelihood_fn = fn
        return self

    def set_data(self, data: Any) -> "Model":
        self._data = data
        return self

    # ----- deterministic nodes -----

    def add_deterministic(self, name: str, fn: Callable) -> "Model":
        """Add a deterministic transformation of parameters."""
        self._deterministic[name] = fn
        return self

    # ----- log-probability -----

    def log_prior(self, theta: Dict[str, float]) -> float:
        """Compute log p(theta) = sum of log priors."""
        lp = 0.0
        for name in self._param_order:
            val = theta[name]
            if not np.isfinite(val):
                return -math.inf
            lp += self.parameters[name].log_prior(val)
            if not np.isfinite(lp):
                return -math.inf
        return lp

    def log_likelihood(self, theta: Dict[str, float]) -> float:
        """Compute log p(data | theta)."""
        if self._log_likelihood_fn is None:
            raise RuntimeError("No likelihood function set")
        return self._log_likelihood_fn(self._data, **theta)

    def log_posterior(self, theta: Dict[str, float]) -> float:
        """Compute log p(theta | data) ∝ log p(data|theta) + log p(theta)."""
        lp = self.log_prior(theta)
        if not math.isfinite(lp):
            return -math.inf
        ll = self.log_likelihood(theta)
        if not math.isfinite(ll):
            return -math.inf
        return lp + ll

    # ----- initial values -----

    def initial_theta(self, rng: np.random.Generator) -> Dict[str, float]:
        """Draw initial parameter values from their priors."""
        return {name: self.parameters[name].sample_from_prior(rng)
                for name in self._param_order}

    def validate_theta(self, theta: Dict[str, float]) -> bool:
        """Check that all required parameters are present and finite."""
        for name in self._param_order:
            if name not in theta:
                return False
            if not np.isfinite(theta[name]):
                return False
        return True

    # ----- convenience: common models -----

    @classmethod
    def linear_regression(
        cls,
        X: np.ndarray,
        y: np.ndarray,
        sigma_prior: float = 10.0,
        noise_prior_alpha: float = 1.0,
        noise_prior_beta: float = 1.0,
    ) -> "Model":
        """
        Bayesian linear regression: y = X @ beta + eps, eps ~ N(0, sigma^2).

        Priors:
            beta_j ~ N(0, sigma_prior^2)
            sigma  ~ HalfNormal(sigma_prior) via Gamma noise prior
        """
        X = np.asarray(X, dtype=float)
        y = np.asarray(y, dtype=float)
        k = X.shape[1] if X.ndim > 1 else 1
        if X.ndim == 1:
            X = X.reshape(-1, 1)

        model = cls(name="linear_regression")
        for j in range(k):
            model.add_parameter(f"beta_{j}", Normal(0, sigma_prior))
        model.add_parameter("sigma", Gamma(noise_prior_alpha, noise_prior_beta))

        def log_lik(data, **params):
            betas = np.array([params[f"beta_{j}"] for j in range(k)])
            sigma = params["sigma"]
            if sigma <= 0:
                return -math.inf
            mu = X @ betas
            residuals = data - mu
            n = len(residuals)
            return -0.5 * n * math.log(2 * math.pi * sigma**2) - 0.5 * np.sum(residuals**2) / sigma**2

        model.set_likelihood(log_lik)
        model.set_data(y)
        return model

    @classmethod
    def beta_binomial(
        cls,
        alpha_prior: float = 1.0,
        beta_prior: float = 1.0,
    ) -> "Model":
        """Beta-binomial: p ~ Beta(a, b), data ~ Binomial(n, p)."""
        model = cls(name="beta_binomial")
        model.add_parameter("p", Beta(alpha_prior, beta_prior))

        def log_lik(data, **params):
            p = params["p"]
            if p <= 0 or p >= 1:
                return -math.inf
            # data is array of 0/1 or successes; treat as list of Bernoulli trials
            data = np.asarray(data, dtype=float)
            k = data.sum()
            n = len(data)
            return k * math.log(p) + (n - k) * math.log(1 - p)

        model.set_likelihood(log_lik)
        return model


# ---------------------------------------------------------------------------
# Built-in likelihood functions
# ---------------------------------------------------------------------------

def normal_likelihood(data: np.ndarray, mu: float, sigma: float) -> float:
    """Log-likelihood for i.i.d. normal observations."""
    data = np.asarray(data, dtype=float)
    if sigma <= 0:
        return -math.inf
    n = len(data)
    return -0.5 * n * math.log(2 * math.pi * sigma**2) - 0.5 * np.sum((data - mu) ** 2) / sigma**2


def poisson_likelihood(data: np.ndarray, lam: float) -> float:
    """Log-likelihood for i.i.d. Poisson observations."""
    if lam <= 0:
        return -math.inf
    data = np.asarray(data, dtype=float)
    return float(np.sum(data * math.log(lam) - lam - np.vectorize(math.lgamma)(data + 1)))
