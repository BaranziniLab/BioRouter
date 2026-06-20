"""Tests for MCMC diagnostics."""

import math
import pytest
import numpy as np
from numpy.testing import assert_allclose

from bayesmcmc.diagnostics import (
    compute_ess,
    compute_rhat,
    autocorrelation,
    trace_summary,
    geweke_diagnostic,
    burn_in,
    thin,
    multi_chain_summary,
)


class TestESS:
    def test_ess_independent(self):
        """ESS of independent samples should be close to n."""
        rng = np.random.default_rng(42)
        samples = rng.normal(0, 1, size=10000)
        ess = compute_ess(samples)
        # for iid samples, ESS ≈ n
        assert ess > 5000, f"ESS of iid samples should be high, got {ess}"

    def test_ess_correlated(self):
        """ESS of highly correlated samples should be much lower than n."""
        rng = np.random.default_rng(42)
        n = 10000
        # random walk: highly autocorrelated
        samples = np.cumsum(rng.normal(0, 1, size=n))
        ess = compute_ess(samples)
        assert ess < n * 0.5, f"ESS of random walk should be lower, got {ess}"

    def test_ess_short_chain(self):
        """ESS of very short chain should return n."""
        ess = compute_ess(np.array([1.0, 2.0, 3.0]))
        assert ess == 3.0

    def test_ess_constant(self):
        """ESS of constant chain should be handled gracefully."""
        ess = compute_ess(np.ones(100))
        assert ess >= 1.0


class TestRhat:
    def test_rhat_converged(self):
        """R-hat of identical chains should be 1.0."""
        chain = np.random.default_rng(42).normal(0, 1, size=(4, 1000))
        rhat = compute_rhat(chain)
        assert_allclose(rhat, 1.0, atol=0.05)

    def test_rhat_different_means(self):
        """R-hat of chains with very different means should be > 1."""
        rng = np.random.default_rng(42)
        chain1 = rng.normal(0, 1, size=(1, 1000))
        chain2 = rng.normal(10, 1, size=(1, 1000))
        chains = np.vstack([chain1, chain2])
        rhat = compute_rhat(chains)
        assert rhat > 1.5, f"R-hat for different chains should be > 1.5, got {rhat}"

    def test_rhat_dict_input(self):
        """R-hat should work with dict input."""
        rng = np.random.default_rng(42)
        chains = {
            "mu": rng.normal(0, 1, size=(4, 1000)),
            "sigma": rng.gamma(2, 1, size=(4, 1000)),
        }
        rhat_mu = compute_rhat(chains, "mu")
        rhat_sigma = compute_rhat(chains, "sigma")
        assert 0.9 < rhat_mu < 1.1
        assert 0.9 < rhat_sigma < 1.1

    def test_rhat_single_chain(self):
        """R-hat with single chain should return 1.0."""
        chain = np.random.default_rng(42).normal(0, 1, size=(1, 100))
        rhat = compute_rhat(chain)
        assert rhat == 1.0


class TestAutocorrelation:
    def test_acf_lag0(self):
        """ACF at lag 0 should be 1.0."""
        rng = np.random.default_rng(42)
        samples = rng.normal(0, 1, size=1000)
        acf = autocorrelation(samples)
        assert_allclose(acf[0], 1.0, atol=1e-10)

    def test_acf_independent(self):
        """ACF of independent samples should be near 0 for lag > 0."""
        rng = np.random.default_rng(42)
        samples = rng.normal(0, 1, size=10000)
        acf = autocorrelation(samples, max_lag=20)
        # for large n, lag > 0 autocorrelations should be small
        assert np.abs(acf[1:]).max() < 0.15

    def test_acf_length(self):
        samples = np.random.default_rng(42).normal(0, 1, size=100)
        acf = autocorrelation(samples, max_lag=10)
        assert len(acf) == 11  # lags 0..10


class TestTraceSummary:
    def test_basic_stats(self):
        samples = np.array([1.0, 2.0, 3.0, 4.0, 5.0])
        s = trace_summary(samples)
        assert_allclose(s["mean"], 3.0)
        assert_allclose(s["std"], np.std(samples, ddof=1), atol=1e-10)
        assert_allclose(s["min"], 1.0)
        assert_allclose(s["max"], 5.0)
        assert s["n"] == 5

    def test_quantiles(self):
        samples = np.arange(101, dtype=float)  # 0..100
        s = trace_summary(samples, quantiles=[0.5])
        assert_allclose(s["q0.500"], 50.0, atol=0.5)


class TestGeweke:
    def test_converged_chain(self):
        """Geweke z-score for converged chain should be small."""
        rng = np.random.default_rng(42)
        samples = rng.normal(0, 1, size=5000)
        z = geweke_diagnostic(samples)
        assert abs(z) < 2.0, f"Geweke z={z} should be < 2 for converged chain"

    def test_short_chain(self):
        """Geweke for short chain should return 0."""
        z = geweke_diagnostic(np.array([1.0, 2.0, 3.0]))
        assert z == 0.0


class TestBurnIn:
    def test_burn_in(self):
        chains = {
            "mu": np.arange(200).reshape(2, 100).astype(float),
            "_acceptance_rate": np.array([0.5, 0.5]),
        }
        result = burn_in(chains, 30)
        assert result["mu"].shape == (2, 70)
        assert "_acceptance_rate" not in result


class TestThin:
    def test_thin(self):
        chains = {
            "mu": np.arange(200).reshape(2, 100).astype(float),
            "_acceptance_rate": np.array([0.5, 0.5]),
        }
        result = thin(chains, 5)
        assert result["mu"].shape == (2, 20)
        assert "_acceptance_rate" not in result


class TestMultiChainSummary:
    def test_basic(self):
        rng = np.random.default_rng(42)
        chains = {
            "mu": rng.normal(5, 1, size=(4, 2000)),
            "sigma": rng.gamma(2, 1, size=(4, 2000)),
        }
        summaries = multi_chain_summary(chains)
        assert "mu" in summaries
        assert "sigma" in summaries
        assert_allclose(summaries["mu"]["mean"], 5.0, atol=0.2)
        assert "rhat" in summaries["mu"]
        assert "ess" in summaries["mu"]
