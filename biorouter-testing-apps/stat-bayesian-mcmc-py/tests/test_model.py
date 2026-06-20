"""Tests for model specification API."""

import math
import pytest
import numpy as np
from numpy.testing import assert_allclose

from bayesmcmc.model import Model, Parameter
from bayesmcmc.distributions import Normal, Beta, Gamma


class TestParameter:
    def test_initialization(self):
        p = Parameter("mu", Normal(0, 1))
        assert p.name == "mu"
        assert np.isfinite(p.initial_value)

    def test_log_prior(self):
        p = Parameter("mu", Normal(0, 1))
        # log pdf of N(0,1) at 0
        expected = -0.5 * math.log(2 * math.pi)
        assert_allclose(p.log_prior(0.0), expected, atol=1e-10)

    def test_fixed_parameter(self):
        p = Parameter("mu", Normal(0, 1), initial_value=5.0, fixed=True)
        assert p.log_prior(5.0) == 0.0
        assert p.log_prior(0.0) == -math.inf

    def test_sample_from_prior(self):
        rng = np.random.default_rng(42)
        p = Parameter("mu", Normal(5, 0.1))
        val = p.sample_from_prior(rng)
        assert np.isfinite(val)
        assert abs(val - 5) < 1  # very unlikely to be far from mean


class TestModel:
    def test_add_parameter(self):
        model = Model()
        model.add_parameter("mu", Normal(0, 1))
        model.add_parameter("sigma", Gamma(2, 2))
        assert model.get_parameter_names() == ["mu", "sigma"]

    def test_log_prior(self):
        model = Model()
        model.add_parameter("mu", Normal(0, 1))
        theta = {"mu": 0.0}
        expected = -0.5 * math.log(2 * math.pi)
        assert_allclose(model.log_prior(theta), expected, atol=1e-10)

    def test_log_prior_infinite(self):
        model = Model()
        model.add_parameter("mu", Normal(0, 1))
        theta = {"mu": float("inf")}
        assert model.log_prior(theta) == -math.inf

    def test_log_likelihood(self):
        model = Model()
        model.add_parameter("mu", Normal(0, 10))
        data = np.array([1.0, 2.0, 3.0])

        def log_lik(data, mu):
            return -0.5 * np.sum((data - mu) ** 2)

        model.set_likelihood(log_lik)
        model.set_data(data)
        theta = {"mu": 2.0}
        expected = -0.5 * ((1 - 2) ** 2 + (2 - 2) ** 2 + (3 - 2) ** 2)
        assert_allclose(model.log_likelihood(theta), expected, atol=1e-10)

    def test_log_posterior(self):
        model = Model()
        model.add_parameter("mu", Normal(0, 1))
        data = np.array([1.0])

        def log_lik(data, mu):
            return -0.5 * (data[0] - mu) ** 2

        model.set_likelihood(log_lik)
        model.set_data(data)
        theta = {"mu": 0.0}
        # log_post = log_prior(0|N(0,1)) + log_lik(1|mu=0)
        expected = -0.5 * math.log(2 * math.pi) + -0.5 * 1.0
        assert_allclose(model.log_posterior(theta), expected, atol=1e-10)

    def test_log_posterior_no_likelihood(self):
        model = Model()
        model.add_parameter("mu", Normal(0, 1))
        with pytest.raises(RuntimeError):
            model.log_likelihood({"mu": 0.0})

    def test_initial_theta(self):
        rng = np.random.default_rng(42)
        model = Model()
        model.add_parameter("mu", Normal(5, 0.1))
        model.add_parameter("sigma", Gamma(2, 2))
        theta = model.initial_theta(rng)
        assert "mu" in theta
        assert "sigma" in theta
        assert np.isfinite(theta["mu"])
        assert np.isfinite(theta["sigma"])

    def test_validate_theta(self):
        model = Model()
        model.add_parameter("mu", Normal(0, 1))
        model.add_parameter("sigma", Gamma(1, 1))
        assert model.validate_theta({"mu": 0.0, "sigma": 1.0})
        assert not model.validate_theta({"mu": 0.0})
        assert not model.validate_theta({"mu": float("nan"), "sigma": 1.0})

    def test_linear_regression_model(self):
        rng = np.random.default_rng(42)
        X = rng.uniform(-1, 1, size=(20, 2))
        beta = np.array([1.0, 2.0])
        y = X @ beta + rng.normal(0, 0.1, size=20)

        model = Model.linear_regression(X, y)
        assert model.get_parameter_names() == ["beta_0", "beta_1", "sigma"]

        theta = {"beta_0": 1.0, "beta_1": 2.0, "sigma": 0.1}
        lp = model.log_posterior(theta)
        assert math.isfinite(lp)

    def test_beta_binomial_model(self):
        model = Model.beta_binomial()
        assert model.get_parameter_names() == ["p"]

        theta = {"p": 0.7}
        data = np.array([1, 1, 1, 0, 0])
        model.set_data(data)
        lp = model.log_posterior(theta)
        assert math.isfinite(lp)
