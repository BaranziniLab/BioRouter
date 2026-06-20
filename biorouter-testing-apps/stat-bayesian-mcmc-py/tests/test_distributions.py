"""Tests for probability distributions."""

import math
import pytest
import numpy as np
from numpy.testing import assert_allclose

from bayesmcmc.distributions import (
    Normal,
    Bernoulli,
    Binomial,
    Poisson,
    Gamma,
    Beta,
    Uniform,
    StudentT,
)


class TestNormal:
    def test_log_pdf_standard(self):
        """Standard normal log-pdf at 0 should be -0.5*log(2pi)."""
        n = Normal(0, 1)
        expected = -0.5 * math.log(2 * math.pi)
        assert_allclose(n.log_pdf(0.0), expected, atol=1e-10)

    def test_log_pdf_values(self):
        n = Normal(0, 1)
        # N(0,1) at x=1
        expected = -0.5 * (1.0 ** 2) - 0.5 * math.log(2 * math.pi)
        assert_allclose(n.log_pdf(1.0), expected, atol=1e-10)

    def test_log_pdf_custom_params(self):
        n = Normal(0, 1)
        # N(2, 3) at x=2 -> mean, so log-pdf = -0.5*log(2pi) - log(3)
        expected = -0.5 * math.log(2 * math.pi) - math.log(3)
        assert_allclose(n.log_pdf(2.0, mu=2, sigma=3), expected, atol=1e-10)

    def test_sample_shape(self):
        n = Normal(0, 1)
        rng = np.random.default_rng(42)
        samples = n.sample(100, rng)
        assert samples.shape == (100,)
        assert np.isfinite(samples).all()

    def test_sample_mean(self):
        n = Normal(5.0, 0.5)
        rng = np.random.default_rng(42)
        samples = n.sample(10000, rng)
        assert_allclose(samples.mean(), 5.0, atol=0.1)
        assert_allclose(samples.std(), 0.5, atol=0.05)

    def test_posterior_update(self):
        """Conjugate normal-normal update with known likelihood sigma."""
        n = Normal(0, 10)  # prior N(0, 10^2)
        data = np.array([1.0, 1.1, 0.9, 1.0, 0.95])
        post = n.posterior_update(data, likelihood_sigma=1.0)
        # analytical:
        # post_prec = 1/100 + 5/1 = 5.01
        # post_var = 1/5.01
        # x_bar = mean(data) = 4.95/5 = 0.99
        # post_mean = post_var * (0/100 + 5*x_bar/1) = post_var * 5*x_bar
        x_bar = data.mean()
        expected_mean = (1.0 / 5.01) * 5.0 * x_bar
        expected_var = 1.0 / 5.01
        assert_allclose(post["mu"], expected_mean, atol=1e-6)
        assert_allclose(post["sigma"], math.sqrt(expected_var), atol=1e-6)

    def test_invalid_sigma(self):
        with pytest.raises(ValueError):
            Normal(0, -1)


class TestBernoulli:
    def test_log_pdf(self):
        b = Bernoulli(0.7)
        assert_allclose(b.log_pdf(1.0), math.log(0.7), atol=1e-10)
        assert_allclose(b.log_pdf(0.0), math.log(0.3), atol=1e-10)

    def test_log_pdf_invalid(self):
        b = Bernoulli(0.5)
        assert b.log_pdf(0.5) == -math.inf

    def test_sample(self):
        b = Bernoulli(0.5)
        rng = np.random.default_rng(42)
        samples = b.sample(1000, rng)
        assert set(np.unique(samples)).issubset({0.0, 1.0})
        assert_allclose(samples.mean(), 0.5, atol=0.1)


class TestBinomial:
    def test_log_pdf(self):
        b = Binomial(10, 0.5)
        # C(10,5) * 0.5^5 * 0.5^5
        expected = math.lgamma(11) - 2 * math.lgamma(6) + 5 * math.log(0.5) + 5 * math.log(0.5)
        assert_allclose(b.log_pdf(5), expected, atol=1e-10)

    def test_log_pdf_out_of_range(self):
        b = Binomial(10, 0.5)
        assert b.log_pdf(-1) == -math.inf
        assert b.log_pdf(11) == -math.inf

    def test_sample(self):
        b = Binomial(10, 0.5)
        rng = np.random.default_rng(42)
        samples = b.sample(1000, rng)
        assert samples.min() >= 0
        assert samples.max() <= 10
        assert_allclose(samples.mean(), 5.0, atol=0.5)


class TestPoisson:
    def test_log_pdf(self):
        p = Poisson(3.0)
        # P(X=2) = e^{-3} * 3^2 / 2!
        expected = 2 * math.log(3) - 3 - math.lgamma(3)
        assert_allclose(p.log_pdf(2), expected, atol=1e-10)

    def test_log_pdf_negative(self):
        p = Poisson(1.0)
        assert p.log_pdf(-1) == -math.inf

    def test_sample(self):
        p = Poisson(5.0)
        rng = np.random.default_rng(42)
        samples = p.sample(1000, rng)
        assert samples.min() >= 0
        assert_allclose(samples.mean(), 5.0, atol=0.5)


class TestGamma:
    def test_log_pdf(self):
        g = Gamma(2, 1)
        # Gamma(2,1) at x=1: (2-1)*log(1) - 1*1 + 2*log(1) - lgamma(2) = -1
        assert_allclose(g.log_pdf(1.0), -1.0, atol=1e-10)

    def test_log_pdf_invalid(self):
        g = Gamma(1, 1)
        assert g.log_pdf(0) == -math.inf
        assert g.log_pdf(-1) == -math.inf

    def test_sample(self):
        g = Gamma(3, 2)
        rng = np.random.default_rng(42)
        samples = g.sample(10000, rng)
        assert samples.min() > 0
        assert_allclose(samples.mean(), 3 / 2, atol=0.1)


class TestBeta:
    def test_log_pdf_uniform(self):
        """Beta(1,1) is Uniform(0,1), log-pdf = 0."""
        b = Beta(1, 1)
        assert_allclose(b.log_pdf(0.5), 0.0, atol=1e-10)

    def test_log_pdf_invalid(self):
        b = Beta(2, 2)
        assert b.log_pdf(0) == -math.inf
        assert b.log_pdf(1) == -math.inf
        assert b.log_pdf(-0.1) == -math.inf

    def test_sample(self):
        b = Beta(2, 5)
        rng = np.random.default_rng(42)
        samples = b.sample(10000, rng)
        assert samples.min() > 0
        assert samples.max() < 1
        # mean of Beta(2,5) = 2/7
        assert_allclose(samples.mean(), 2 / 7, atol=0.05)

    def test_posterior_update(self):
        """Conjugate Beta-Binomial update."""
        b = Beta(1, 1)  # uniform prior
        data = np.array([1, 1, 1, 0, 0])
        post = b.posterior_update(data)
        assert_allclose(post["a"], 4.0)
        assert_allclose(post["b"], 3.0)


class TestUniform:
    def test_log_pdf(self):
        u = Uniform(0, 1)
        assert_allclose(u.log_pdf(0.5), 0.0, atol=1e-10)
        assert u.log_pdf(-0.1) == -math.inf
        assert u.log_pdf(1.1) == -math.inf

    def test_sample(self):
        u = Uniform(0, 1)
        rng = np.random.default_rng(42)
        samples = u.sample(1000, rng)
        assert samples.min() >= 0
        assert samples.max() <= 1


class TestStudentT:
    def test_log_pdf_standard(self):
        """Student-t with nu=1 is Cauchy."""
        t = StudentT(nu=1, mu=0, sigma=1)
        # Cauchy at 0: log(1/pi)
        expected = -math.log(math.pi)
        assert_allclose(t.log_pdf(0.0), expected, atol=1e-10)

    def test_sample_shape(self):
        t = StudentT(nu=5, mu=0, sigma=1)
        rng = np.random.default_rng(42)
        samples = t.sample(100, rng)
        assert samples.shape == (100,)
        assert np.isfinite(samples).all()

    def test_invalid_params(self):
        with pytest.raises(ValueError):
            StudentT(nu=-1)
        with pytest.raises(ValueError):
            StudentT(nu=1, sigma=-1)
