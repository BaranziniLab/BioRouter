"""Tests for worked example models."""

import math
import pytest
import numpy as np
from numpy.testing import assert_allclose

from bayesmcmc.model import Model
from bayesmcmc.distributions import Normal, Beta, Gamma
from bayesmcmc.samplers import MetropolisHastings, GibbsSampler, SliceSampler
from bayesmcmc.diagnostics import compute_rhat, compute_ess
from bayesmcmc.summary import posterior_summary


class TestBetaBinomial:
    """Test beta-binomial model (conjugate)."""

    def test_conjugate_posterior(self):
        """Analytic posterior should match Beta(alpha+k, beta+n-k)."""
        alpha_prior, beta_prior = 1.0, 1.0
        data = np.array([1, 1, 1, 1, 1, 1, 1, 0, 0, 0])
        k = data.sum()
        n = len(data)

        alpha_post = alpha_prior + k
        beta_post = beta_prior + n - k

        expected_mean = alpha_post / (alpha_post + beta_post)
        expected_var = alpha_post * beta_post / (
            (alpha_post + beta_post) ** 2 * (alpha_post + beta_post + 1)
        )

        model = Model.beta_binomial(alpha_prior, beta_prior)
        model.set_data(data)

        sampler = MetropolisHastings(model, step_sizes={"p": 0.1})
        chains = sampler.run(n_samples=10000, n_chains=4, burn_in=2000, seed=42)

        pooled = chains["p"].flatten()
        assert_allclose(pooled.mean(), expected_mean, atol=0.02)
        assert_allclose(pooled.var(), expected_var, atol=0.01)

    def test_gibbs_recovers_analytic(self):
        """Gibbs sampler with Beta full conditional should match analytic."""
        alpha_prior, beta_prior = 1.0, 1.0
        data = np.array([1, 1, 1, 1, 1, 1, 1, 0, 0, 0])

        model = Model.beta_binomial(alpha_prior, beta_prior)
        model.set_data(data)

        full_cond = GibbsSampler.beta_binomial_conditionals(alpha_prior, beta_prior)
        sampler = GibbsSampler(model, full_conditionals=full_cond)
        chains = sampler.run(n_samples=10000, n_chains=4, burn_in=2000, seed=42)

        expected_mean = 8.0 / 12.0
        pooled = chains["p"].flatten()
        assert_allclose(pooled.mean(), expected_mean, atol=0.02)

    def test_slice_recovers_analytic(self):
        """Slice sampler should recover analytic posterior."""
        data = np.array([1, 1, 1, 1, 1, 1, 1, 0, 0, 0])
        model = Model.beta_binomial(1.0, 1.0)
        model.set_data(data)

        sampler = SliceSampler(model, width=0.3)
        chains = sampler.run(n_samples=10000, n_chains=4, burn_in=2000, seed=42)

        expected_mean = 8.0 / 12.0
        pooled = chains["p"].flatten()
        assert_allclose(pooled.mean(), expected_mean, atol=0.03)


class TestBayesianLinearRegression:
    """Test Bayesian linear regression model."""

    def test_recovers_parameters(self):
        """Should recover known regression coefficients."""
        rng = np.random.default_rng(42)
        n = 100
        x = rng.uniform(-2, 2, size=n)
        true_b0, true_b1, true_sigma = 2.0, 3.0, 0.5
        y = true_b0 + true_b1 * x + rng.normal(0, true_sigma, size=n)

        X = np.column_stack([np.ones(n), x])
        model = Model.linear_regression(X, y, sigma_prior=10.0, noise_prior_alpha=2.0, noise_prior_beta=2.0)

        # Run with fixed proposals (no adaptation) for reliable convergence
        sampler = MetropolisHastings(
            model,
            step_sizes={"beta_0": 0.3, "beta_1": 0.3, "sigma": 0.2},
        )
        chains = sampler.run(
            n_samples=10000, n_chains=4, burn_in=3000, thin=2,
            seed=42, adapt=False,
        )

        b0_samples = chains["beta_0"].flatten()
        b1_samples = chains["beta_1"].flatten()

        assert_allclose(b0_samples.mean(), true_b0, atol=0.5)
        assert_allclose(b1_samples.mean(), true_b1, atol=0.5)

        # R-hat should indicate convergence
        assert compute_rhat(chains, "beta_0") < 1.2
        assert compute_rhat(chains, "beta_1") < 1.2

    def test_conjugate_normal_normal(self):
        """Normal-Normal conjugate model should have analytic posterior."""
        mu_prior_mean = 0.0
        mu_prior_var = 100.0
        sigma_known = 1.0
        data = np.array([1.0, 1.1, 0.9, 1.0, 0.95])

        n = len(data)
        x_bar = data.mean()

        # analytic posterior
        post_var = 1.0 / (1.0 / mu_prior_var + n / sigma_known)
        post_mean = post_var * (mu_prior_mean / mu_prior_var + n * x_bar / sigma_known)

        model = Model()
        model.add_parameter("mu", Normal(mu_prior_mean, math.sqrt(mu_prior_var)))

        def log_lik(d, mu):
            return -0.5 * n * math.log(2 * math.pi * sigma_known**2) - 0.5 * np.sum((d - mu)**2) / sigma_known**2

        model.set_likelihood(log_lik)
        model.set_data(data)

        sampler = MetropolisHastings(model, step_sizes={"mu": 0.5})
        chains = sampler.run(n_samples=5000, n_chains=4, burn_in=1000, seed=42)

        pooled = chains["mu"].flatten()
        assert_allclose(pooled.mean(), post_mean, atol=0.1)


class TestHierarchicalNormal:
    """Test hierarchical normal model."""

    def test_model_runs(self):
        """Hierarchical model should run without error and produce finite samples."""
        rng = np.random.default_rng(42)
        n_groups = 3
        n_per = 20
        true_mu = 5.0
        true_tau = 1.0
        true_sigma = 0.5

        true_thetas = rng.normal(true_mu, true_tau, size=n_groups)
        y = np.array([rng.normal(true_thetas[j], true_sigma, size=n_per) for j in range(n_groups)])

        model = Model(name="hierarchical")
        model.add_parameter("mu", Normal(0, 10))
        model.add_parameter("tau", Gamma(2, 0.5))
        model.add_parameter("sigma", Gamma(2, 0.5))
        for j in range(n_groups):
            model.add_parameter(f"theta_{j}", Normal(0, 10))

        def log_lik(data, **params):
            mu = params["mu"]
            tau = params["tau"]
            sigma = params["sigma"]
            if sigma <= 0 or tau <= 0:
                return -math.inf
            lp = -0.5 * (mu / 10) ** 2
            lp += (2 - 1) * math.log(tau) - 0.5 * tau - math.lgamma(2)
            lp += (2 - 1) * math.log(sigma) - 0.5 * sigma - math.lgamma(2)
            thetas = np.array([params[f"theta_{j}"] for j in range(n_groups)])
            lp += np.sum(-0.5 * ((thetas - mu) / tau) ** 2 - math.log(tau))
            for j in range(n_groups):
                lp += np.sum(-0.5 * ((data[j] - thetas[j]) / sigma) ** 2 - math.log(sigma))
            return lp

        model.set_likelihood(log_lik)
        model.set_data(y)

        step_sizes = {"mu": 0.3, "tau": 0.2, "sigma": 0.2}
        for j in range(n_groups):
            step_sizes[f"theta_{j}"] = 0.3

        sampler = MetropolisHastings(model, step_sizes=step_sizes)
        chains = sampler.run(n_samples=5000, n_chains=3, burn_in=1500, seed=42)

        # check that all chains produced finite samples
        for name in ["mu", "tau", "sigma"] + [f"theta_{j}" for j in range(n_groups)]:
            samples = chains[name].flatten()
            assert np.all(np.isfinite(samples)), f"Non-finite samples for {name}"
            assert len(samples) > 0

        # mu posterior should be finite
        mu_samples = chains["mu"].flatten()
        assert np.isfinite(mu_samples.mean())
