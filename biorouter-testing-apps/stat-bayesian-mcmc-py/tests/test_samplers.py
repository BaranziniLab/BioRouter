"""Tests for MCMC samplers."""

import math
import pytest
import numpy as np
from numpy.testing import assert_allclose

from bayesmcmc.model import Model
from bayesmcmc.distributions import Normal, Beta, Gamma
from bayesmcmc.samplers import MetropolisHastings, GibbsSampler, HMCSampler, SliceSampler
from bayesmcmc.diagnostics import compute_rhat, compute_ess


# ---------------------------------------------------------------------------
# Helper: create a simple normal posterior for testing
# ---------------------------------------------------------------------------

def make_normal_model(mu_prior=0.0, sigma_prior=10.0, known_sigma=1.0):
    """Model: mu ~ N(mu_prior, sigma_prior^2), data ~ N(mu, known_sigma^2)."""
    model = Model(name="test_normal")
    model.add_parameter("mu", Normal(mu_prior, sigma_prior))

    def log_lik(data, mu):
        data = np.asarray(data, dtype=float)
        n = len(data)
        return -0.5 * n * math.log(2 * math.pi * known_sigma**2) - 0.5 * np.sum((data - mu)**2) / known_sigma**2

    model.set_likelihood(log_lik)
    return model


def make_beta_binomial_model():
    """Model: p ~ Beta(1,1), data ~ Bernoulli(p)."""
    model = Model(name="test_bb")
    model.add_parameter("p", Beta(1, 1))

    def log_lik(data, p):
        data = np.asarray(data, dtype=float)
        if p <= 0 or p >= 1:
            return -math.inf
        k = data.sum()
        n = len(data)
        return k * math.log(p) + (n - k) * math.log(1 - p)

    model.set_likelihood(log_lik)
    return model


# ---------------------------------------------------------------------------
# Metropolis-Hastings tests
# ---------------------------------------------------------------------------

class TestMetropolisHastings:
    def test_beta_binomial_conjugate(self):
        """MH should recover Beta posterior for binomial data."""
        model = make_beta_binomial_model()
        data = np.array([1, 1, 1, 1, 1, 1, 1, 0, 0, 0])
        model.set_data(data)

        sampler = MetropolisHastings(model, step_sizes={"p": 0.1})
        chains = sampler.run(n_samples=5000, n_chains=4, burn_in=1000, seed=42)

        # analytic posterior: Beta(8, 4), mean = 8/12 = 0.6667
        pooled = chains["p"].flatten()
        assert_allclose(pooled.mean(), 8 / 12, atol=0.05)

    def test_normal_conjugate(self):
        """MH should recover Normal posterior for normal data."""
        model = make_normal_model(mu_prior=0.0, sigma_prior=10.0, known_sigma=1.0)
        data = np.array([1.0, 1.2, 0.8, 1.1, 0.9])
        model.set_data(data)

        sampler = MetropolisHastings(model, step_sizes={"mu": 0.5})
        chains = sampler.run(n_samples=5000, n_chains=4, burn_in=1000, seed=42)

        # analytic posterior mean = data.mean() = 1.0 (with flat prior)
        pooled = chains["mu"].flatten()
        assert_allclose(pooled.mean(), 1.0, atol=0.1)

    def test_rhat_converged(self):
        """R-hat should be close to 1 for converged chains."""
        model = make_beta_binomial_model()
        data = np.array([1, 1, 1, 0, 0, 1, 1, 0, 1, 1])
        model.set_data(data)

        sampler = MetropolisHastings(model, step_sizes={"p": 0.1})
        chains = sampler.run(n_samples=5000, n_chains=4, burn_in=1000, seed=42)

        rhat = compute_rhat(chains, "p")
        assert rhat < 1.1, f"R-hat should be < 1.1, got {rhat}"

    def test_acceptance_rate(self):
        """Acceptance rate should be reasonable (0.2-0.7)."""
        model = make_beta_binomial_model()
        data = np.array([1, 1, 1, 0, 0])
        model.set_data(data)

        sampler = MetropolisHastings(model, step_sizes={"p": 0.1})
        chains = sampler.run(n_samples=3000, n_chains=2, burn_in=500, seed=42)

        rate = chains["_acceptance_rate"].mean()
        assert 0.15 < rate < 0.85, f"Acceptance rate {rate} outside reasonable range"

    def test_reproducibility(self):
        """Same seed should give same results."""
        model = make_beta_binomial_model()
        data = np.array([1, 1, 0, 1, 1])
        model.set_data(data)

        sampler1 = MetropolisHastings(model, step_sizes={"p": 0.1})
        chains1 = sampler1.run(n_samples=2000, n_chains=2, seed=123)

        sampler2 = MetropolisHastings(model, step_sizes={"p": 0.1})
        chains2 = sampler2.run(n_samples=2000, n_chains=2, seed=123)

        np.testing.assert_array_equal(chains1["p"], chains2["p"])

    def test_ess_positive(self):
        """ESS should be positive."""
        model = make_beta_binomial_model()
        data = np.array([1, 1, 1, 0, 0])
        model.set_data(data)

        sampler = MetropolisHastings(model, step_sizes={"p": 0.1})
        chains = sampler.run(n_samples=3000, n_chains=2, burn_in=500, seed=42)

        ess = compute_ess(chains["p"].flatten())
        assert ess > 0


# ---------------------------------------------------------------------------
# Gibbs sampler tests
# ---------------------------------------------------------------------------

class TestGibbsSampler:
    def test_beta_binomial_with_conditionals(self):
        """Gibbs with Beta full conditional should recover posterior."""
        model = make_beta_binomial_model()
        data = np.array([1, 1, 1, 1, 1, 1, 1, 0, 0, 0])
        model.set_data(data)

        full_cond = GibbsSampler.beta_binomial_conditionals(1.0, 1.0)
        sampler = GibbsSampler(model, full_conditionals=full_cond)
        chains = sampler.run(n_samples=5000, n_chains=4, burn_in=1000, seed=42)

        # analytic: Beta(8, 4), mean = 8/12
        pooled = chains["p"].flatten()
        assert_allclose(pooled.mean(), 8 / 12, atol=0.05)

    def test_normal_with_conditionals(self):
        """Gibbs with Normal full conditional should recover posterior."""
        model = make_normal_model(mu_prior=0.0, sigma_prior=10.0, known_sigma=1.0)
        data = np.array([1.0, 1.2, 0.8, 1.1, 0.9])
        model.set_data(data)

        full_cond = GibbsSampler.normal_normal_conditionals(data, 0.0, 100.0, 1.0)
        sampler = GibbsSampler(model, full_conditionals=full_cond)
        chains = sampler.run(n_samples=5000, n_chains=4, burn_in=1000, seed=42)

        pooled = chains["mu"].flatten()
        assert_allclose(pooled.mean(), 1.0, atol=0.1)

    def test_rhat_converged(self):
        """Gibbs R-hat should be close to 1."""
        model = make_beta_binomial_model()
        data = np.array([1, 1, 1, 0, 0, 1, 1, 0, 1, 1])
        model.set_data(data)

        full_cond = GibbsSampler.beta_binomial_conditionals(1.0, 1.0)
        sampler = GibbsSampler(model, full_conditionals=full_cond)
        chains = sampler.run(n_samples=5000, n_chains=4, burn_in=1000, seed=42)

        rhat = compute_rhat(chains, "p")
        assert rhat < 1.1, f"R-hat should be < 1.1, got {rhat}"


# ---------------------------------------------------------------------------
# Slice sampler tests
# ---------------------------------------------------------------------------

class TestSliceSampler:
    def test_beta_binomial(self):
        """Slice sampler should recover Beta posterior."""
        model = make_beta_binomial_model()
        data = np.array([1, 1, 1, 1, 1, 1, 1, 0, 0, 0])
        model.set_data(data)

        sampler = SliceSampler(model, width=0.3)
        chains = sampler.run(n_samples=5000, n_chains=4, burn_in=1000, seed=42)

        pooled = chains["p"].flatten()
        assert_allclose(pooled.mean(), 8 / 12, atol=0.05)

    def test_normal(self):
        """Slice sampler should recover Normal posterior."""
        model = make_normal_model(mu_prior=0.0, sigma_prior=10.0, known_sigma=1.0)
        data = np.array([1.0, 1.2, 0.8, 1.1, 0.9])
        model.set_data(data)

        sampler = SliceSampler(model, width=1.0)
        chains = sampler.run(n_samples=5000, n_chains=4, burn_in=1000, seed=42)

        pooled = chains["mu"].flatten()
        assert_allclose(pooled.mean(), 1.0, atol=0.1)

    def test_rhat_converged(self):
        model = make_beta_binomial_model()
        data = np.array([1, 1, 1, 0, 0, 1, 1, 0, 1, 1])
        model.set_data(data)

        sampler = SliceSampler(model, width=0.3)
        chains = sampler.run(n_samples=5000, n_chains=4, burn_in=1000, seed=42)

        rhat = compute_rhat(chains, "p")
        assert rhat < 1.15, f"R-hat should be < 1.15, got {rhat}"


# ---------------------------------------------------------------------------
# HMC tests
# ---------------------------------------------------------------------------

class TestHMC:
    def test_normal_posterior(self):
        """HMC should recover Normal posterior."""
        model = make_normal_model(mu_prior=0.0, sigma_prior=10.0, known_sigma=1.0)
        data = np.array([1.0, 1.2, 0.8, 1.1, 0.9, 1.0, 0.95, 1.05])
        model.set_data(data)

        sampler = HMCSampler(model, step_size=0.1, path_length=20)
        chains = sampler.run(n_samples=3000, n_chains=2, burn_in=500, seed=42)

        pooled = chains["mu"].flatten()
        assert_allclose(pooled.mean(), 1.0, atol=0.15)

    def test_acceptance_rate(self):
        """HMC acceptance rate should be reasonable."""
        model = make_normal_model(mu_prior=0.0, sigma_prior=10.0, known_sigma=1.0)
        data = np.array([1.0, 1.2, 0.8])
        model.set_data(data)

        sampler = HMCSampler(model, step_size=0.1, path_length=10)
        chains = sampler.run(n_samples=2000, n_chains=2, burn_in=500, seed=42)

        rate = chains["_acceptance_rate"].mean()
        assert 0.3 < rate < 1.0, f"HMC acceptance rate {rate} outside range"

    def test_reproducibility(self):
        model = make_normal_model(mu_prior=0.0, sigma_prior=10.0, known_sigma=1.0)
        data = np.array([1.0, 1.2, 0.8])
        model.set_data(data)

        sampler1 = HMCSampler(model, step_size=0.1, path_length=10)
        chains1 = sampler1.run(n_samples=1000, n_chains=1, seed=99)

        sampler2 = HMCSampler(model, step_size=0.1, path_length=10)
        chains2 = sampler2.run(n_samples=1000, n_chains=1, seed=99)

        np.testing.assert_array_equal(chains1["mu"], chains2["mu"])

    def test_gradient_computation(self):
        """Test that numerical gradients are reasonable."""
        model = make_normal_model(mu_prior=0.0, sigma_prior=1.0, known_sigma=1.0)
        data = np.array([1.0])
        model.set_data(data)

        sampler = HMCSampler(model)

        # gradient at mu=0 should be positive (pulling toward data=1)
        grad = sampler._grad_log_prob(np.array([0.0]))
        assert grad[0] > 0, "Gradient at mu=0 should point toward data"

        # gradient at mu=2 should be negative (pulling back toward data=1)
        grad = sampler._grad_log_prob(np.array([2.0]))
        assert grad[0] < 0, "Gradient at mu=2 should point back toward data"
