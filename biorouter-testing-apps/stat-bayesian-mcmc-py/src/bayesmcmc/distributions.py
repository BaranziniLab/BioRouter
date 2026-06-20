"""
Probability distributions for Bayesian inference.

Each distribution provides:
- log_pdf(x, **params): log probability density/mass function
- sample(n, rng): random sampling
- posterior_update(data): conjugate posterior update (where available)
"""

from __future__ import annotations

import math
from typing import Optional, Tuple, Union

import numpy as np
from numpy.random import Generator


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

LOG2PI = math.log(2.0 * math.pi)
LOG2 = math.log(2.0)


def _normalised_array(a: np.ndarray) -> np.ndarray:
    return np.asarray(a, dtype=float)


# ---------------------------------------------------------------------------
# Distribution base class
# ---------------------------------------------------------------------------

class Distribution:
    """Abstract base for all distributions."""

    name: str = "Distribution"

    def log_pdf(self, x: Union[float, np.ndarray], **params) -> float:
        raise NotImplementedError

    def sample(self, n: int, rng: Generator) -> np.ndarray:
        raise NotImplementedError

    def posterior_update(self, data: np.ndarray) -> dict:
        raise NotImplementedError(
            f"No conjugate update implemented for {self.name}"
        )


# ---------------------------------------------------------------------------
# Normal (Gaussian)
# ---------------------------------------------------------------------------

class Normal(Distribution):
    """Univariate normal distribution N(mu, sigma^2)."""

    name = "Normal"

    def __init__(self, mu: float = 0.0, sigma: float = 1.0):
        if sigma <= 0:
            raise ValueError("sigma must be positive")
        self.mu = float(mu)
        self.sigma = float(sigma)

    def log_pdf(self, x, **params) -> float:
        mu = params.get("mu", self.mu)
        sigma = params.get("sigma", self.sigma)
        x = _normalised_array(x)
        return float(-0.5 * ((x - mu) / sigma) ** 2 - math.log(sigma) - 0.5 * LOG2PI)

    def sample(self, n: int, rng: Generator) -> np.ndarray:
        return rng.normal(self.mu, self.sigma, size=n)

    def posterior_update(self, data: np.ndarray, likelihood_sigma: float = None) -> dict:
        """Conjugate normal-normal update with known likelihood variance.

        Parameters
        ----------
        data : array-like
            Observed data y_1, ..., y_n ~ N(mu, likelihood_sigma^2).
        likelihood_sigma : float, optional
            Known observation standard deviation. If None, uses self.sigma.
        """
        data = _normalised_array(data)
        n = len(data)
        if n == 0:
            return {"mu": self.mu, "sigma": self.sigma}

        x_bar = data.mean()
        sigma0_sq = self.sigma ** 2  # prior variance
        sigma_sq = (likelihood_sigma if likelihood_sigma is not None else self.sigma) ** 2

        # Posterior precision = prior precision + n / likelihood_var
        post_prec = 1.0 / sigma0_sq + n / sigma_sq
        post_var = 1.0 / post_prec
        post_mu = post_var * (self.mu / sigma0_sq + n * x_bar / sigma_sq)
        post_sigma = math.sqrt(post_var)

        return {"mu": post_mu, "sigma": post_sigma}


# ---------------------------------------------------------------------------
# Multivariate Normal
# ---------------------------------------------------------------------------

class MultivariateNormal(Distribution):
    """Multivariate normal N(mu, Sigma)."""

    name = "MultivariateNormal"

    def __init__(self, mu: np.ndarray, cov: np.ndarray):
        self.mu = np.asarray(mu, dtype=float)
        self.cov = np.asarray(cov, dtype=float)
        self.k = len(self.mu)
        self._cov_inv = np.linalg.inv(self.cov)
        self._log_det = math.log(np.linalg.det(self.cov))

    def log_pdf(self, x, **params) -> float:
        mu = params.get("mu", self.mu)
        cov_inv = params.get("cov_inv", self._cov_inv)
        log_det = params.get("log_det", self._log_det)
        x = np.asarray(x, dtype=float)
        diff = x - mu
        return float(-0.5 * (diff @ cov_inv @ diff + self.k * LOG2PI + log_det))

    def sample(self, n: int, rng: Generator) -> np.ndarray:
        return rng.multivariate_normal(self.mu, self.cov, size=n)


# ---------------------------------------------------------------------------
# Bernoulli
# ---------------------------------------------------------------------------

class Bernoulli(Distribution):
    """Bernoulli distribution Ber(p)."""

    name = "Bernoulli"

    def __init__(self, p: float = 0.5):
        if not 0 <= p <= 1:
            raise ValueError("p must be in [0, 1]")
        self.p = float(p)

    def log_pdf(self, x, **params) -> float:
        p = params.get("p", self.p)
        x = float(x)
        if x not in (0.0, 1.0):
            return -math.inf
        if x == 1.0:
            return math.log(p + 1e-300)
        return math.log(1.0 - p + 1e-300)

    def sample(self, n: int, rng: Generator) -> np.ndarray:
        return rng.binomial(1, self.p, size=n).astype(float)


# ---------------------------------------------------------------------------
# Binomial
# ---------------------------------------------------------------------------

class Binomial(Distribution):
    """Binomial distribution Bin(n, p)."""

    name = "Binomial"

    def __init__(self, n: int = 1, p: float = 0.5):
        if n < 0:
            raise ValueError("n must be non-negative")
        if not 0 <= p <= 1:
            raise ValueError("p must be in [0, 1]")
        self.n = int(n)
        self.p = float(p)

    def log_pdf(self, x, **params) -> float:
        n = params.get("n", self.n)
        p = params.get("p", self.p)
        x = int(x)
        if x < 0 or x > n:
            return -math.inf
        # log C(n,x) + x*log(p) + (n-x)*log(1-p)
        log_binom = (
            math.lgamma(n + 1) - math.lgamma(x + 1) - math.lgamma(n - x + 1)
        )
        return log_binom + x * math.log(p + 1e-300) + (n - x) * math.log(
            1.0 - p + 1e-300
        )

    def sample(self, n: int, rng: Generator) -> np.ndarray:
        return rng.binomial(self.n, self.p, size=n).astype(float)

    def posterior_update(self, data: np.ndarray) -> dict:
        """Conjugate Beta-Binomial update."""
        data = _normalised_array(data)
        # With Beta(a, b) prior on p, observing s successes in n trials:
        # posterior is Beta(a + s, b + n - s)
        a_prior, b_prior = 1.0, 1.0  # default uniform prior
        s = data.sum()
        t = len(data) * self.n
        a_post = a_prior + s
        b_post = b_prior + t - s
        return {"a": a_post, "b": b_post}


# ---------------------------------------------------------------------------
# Poisson
# ---------------------------------------------------------------------------

class Poisson(Distribution):
    """Poisson distribution Pois(lambda)."""

    name = "Poisson"

    def __init__(self, lam: float = 1.0):
        if lam <= 0:
            raise ValueError("lambda must be positive")
        self.lam = float(lam)

    def log_pdf(self, x, **params) -> float:
        lam = params.get("lam", self.lam)
        x = int(x)
        if x < 0:
            return -math.inf
        return x * math.log(lam) - lam - math.lgamma(x + 1)

    def sample(self, n: int, rng: Generator) -> np.ndarray:
        return rng.poisson(self.lam, size=n).astype(float)

    def posterior_update(self, data: np.ndarray) -> dict:
        """Conjugate Gamma-Poisson update."""
        data = _normalised_array(data)
        # Gamma(alpha, beta) prior; observing sum(data) with n observations
        alpha_prior, beta_prior = 1.0, 1.0
        s = data.sum()
        n = len(data)
        alpha_post = alpha_prior + s
        beta_post = beta_prior + n
        return {"alpha": alpha_post, "beta": beta_post}


# ---------------------------------------------------------------------------
# Gamma
# ---------------------------------------------------------------------------

class Gamma(Distribution):
    """Gamma distribution Gamma(alpha, beta) with shape=alpha, rate=beta."""

    name = "Gamma"

    def __init__(self, alpha: float = 1.0, beta: float = 1.0):
        if alpha <= 0 or beta <= 0:
            raise ValueError("alpha and beta must be positive")
        self.alpha = float(alpha)
        self.beta = float(beta)

    def log_pdf(self, x, **params) -> float:
        alpha = params.get("alpha", self.alpha)
        beta = params.get("beta", self.beta)
        x = float(x)
        if x <= 0:
            return -math.inf
        return (
            (alpha - 1) * math.log(x)
            - beta * x
            + alpha * math.log(beta)
            - math.lgamma(alpha)
        )

    def sample(self, n: int, rng: Generator) -> np.ndarray:
        return rng.gamma(self.alpha, 1.0 / self.beta, size=n)


# ---------------------------------------------------------------------------
# Beta
# ---------------------------------------------------------------------------

class Beta(Distribution):
    """Beta distribution Beta(a, b)."""

    name = "Beta"

    def __init__(self, a: float = 1.0, b: float = 1.0):
        if a <= 0 or b <= 0:
            raise ValueError("a and b must be positive")
        self.a = float(a)
        self.b = float(b)

    def log_pdf(self, x, **params) -> float:
        a = params.get("a", self.a)
        b = params.get("b", self.b)
        x = float(x)
        if x <= 0 or x >= 1:
            return -math.inf
        return (
            (a - 1) * math.log(x)
            + (b - 1) * math.log(1.0 - x)
            - math.lgamma(a)
            - math.lgamma(b)
            + math.lgamma(a + b)
        )

    def sample(self, n: int, rng: Generator) -> np.ndarray:
        return rng.beta(self.a, self.b, size=n)

    def posterior_update(self, data: np.ndarray) -> dict:
        """Conjugate Beta update: Beta(a,b) prior, observe k successes, n-k failures."""
        data = _normalised_array(data)
        k = data.sum()
        n = len(data)
        return {"a": self.a + k, "b": self.b + n - k}


# ---------------------------------------------------------------------------
# Uniform
# ---------------------------------------------------------------------------

class Uniform(Distribution):
    """Uniform distribution U(a, b)."""

    name = "Uniform"

    def __init__(self, a: float = 0.0, b: float = 1.0):
        if a >= b:
            raise ValueError("a must be less than b")
        self.a = float(a)
        self.b = float(b)

    def log_pdf(self, x, **params) -> float:
        a = params.get("a", self.a)
        b = params.get("b", self.b)
        x = float(x)
        if a <= x <= b:
            return -math.log(b - a)
        return -math.inf

    def sample(self, n: int, rng: Generator) -> np.ndarray:
        return rng.uniform(self.a, self.b, size=n)


# ---------------------------------------------------------------------------
# Student-t
# ---------------------------------------------------------------------------

class StudentT(Distribution):
    """Student-t distribution with nu degrees of freedom, location mu, scale sigma."""

    name = "StudentT"

    def __init__(self, nu: float = 1.0, mu: float = 0.0, sigma: float = 1.0):
        if nu <= 0:
            raise ValueError("nu must be positive")
        if sigma <= 0:
            raise ValueError("sigma must be positive")
        self.nu = float(nu)
        self.mu = float(mu)
        self.sigma = float(sigma)

    def log_pdf(self, x, **params) -> float:
        nu = params.get("nu", self.nu)
        mu = params.get("mu", self.mu)
        sigma = params.get("sigma", self.sigma)
        x = float(x)
        z = (x - mu) / sigma
        return (
            math.lgamma((nu + 1) / 2)
            - math.lgamma(nu / 2)
            - 0.5 * math.log(nu * math.pi)
            - math.log(sigma)
            - (nu + 1) / 2 * math.log(1 + z ** 2 / nu)
        )

    def sample(self, n: int, rng: Generator) -> np.ndarray:
        return rng.standard_t(self.nu, size=n) * self.sigma + self.mu


# ---------------------------------------------------------------------------
# Distribution registry for CLI / model builder
# ---------------------------------------------------------------------------

DISTRIBUTIONS = {
    "normal": Normal,
    "multivariate_normal": MultivariateNormal,
    "bernoulli": Bernoulli,
    "binomial": Binomial,
    "poisson": Poisson,
    "gamma": Gamma,
    "beta": Beta,
    "uniform": Uniform,
    "student_t": StudentT,
}
