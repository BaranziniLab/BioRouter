"""Tests for posterior summary statistics."""

import math
import pytest
import numpy as np
from numpy.testing import assert_allclose

from bayesmcmc.summary import (
    posterior_mean,
    posterior_median,
    posterior_mode,
    credible_interval,
    hpd_interval,
    quantiles,
    posterior_summary,
    multi_param_summary,
    format_summary_table,
    format_trace_ascii,
    format_histogram_ascii,
)


class TestPosteriorMean:
    def test_basic(self):
        assert_allclose(posterior_mean(np.array([1.0, 2.0, 3.0])), 2.0)

    def test_weighted(self):
        samples = np.array([0.0, 10.0])
        assert_allclose(posterior_mean(samples), 5.0)


class TestPosteriorMedian:
    def test_basic(self):
        assert_allclose(posterior_median(np.array([1.0, 2.0, 3.0])), 2.0)

    def test_even(self):
        assert_allclose(posterior_median(np.array([1.0, 2.0, 3.0, 4.0])), 2.5)


class TestPosteriorMode:
    def test_unimodal(self):
        rng = np.random.default_rng(42)
        samples = rng.normal(5.0, 0.1, size=10000)
        mode = posterior_mode(samples)
        assert abs(mode - 5.0) < 0.5


class TestCredibleInterval:
    def test_95_ci(self):
        rng = np.random.default_rng(42)
        samples = rng.normal(0, 1, size=100000)
        lower, upper = credible_interval(samples, 0.95)
        # 95% CI of N(0,1) should be approx [-1.96, 1.96]
        assert_allclose(lower, -1.96, atol=0.1)
        assert_allclose(upper, 1.96, atol=0.1)

    def test_50_ci(self):
        rng = np.random.default_rng(42)
        samples = rng.normal(0, 1, size=100000)
        lower, upper = credible_interval(samples, 0.50)
        assert lower < 0 < upper
        assert upper - lower < 2.0  # should be narrower than 95% CI


class TestHPDInterval:
    def test_symmetric(self):
        """HPD of symmetric distribution should be centered."""
        rng = np.random.default_rng(42)
        samples = rng.normal(0, 1, size=100000)
        lower, upper = hpd_interval(samples, 0.95)
        # should be roughly centered on 0
        assert abs((lower + upper) / 2) < 0.1

    def test_contains_most_data(self):
        """95% HPD should contain ~95% of samples."""
        rng = np.random.default_rng(42)
        samples = rng.normal(0, 1, size=10000)
        lower, upper = hpd_interval(samples, 0.95)
        frac = np.mean((samples >= lower) & (samples <= upper))
        assert frac >= 0.90, f"HPD should contain ~95% of data, got {frac:.2%}"


class TestQuantiles:
    def test_basic(self):
        samples = np.arange(101, dtype=float)  # 0..100
        q = quantiles(samples, [0.0, 0.5, 1.0])
        assert_allclose(q["q0.000"], 0.0)
        assert_allclose(q["q0.500"], 50.0, atol=0.5)
        assert_allclose(q["q1.000"], 100.0)


class TestPosteriorSummary:
    def test_basic(self):
        rng = np.random.default_rng(42)
        samples = rng.normal(5, 1, size=5000)
        s = posterior_summary(samples)
        assert_allclose(s["mean"], 5.0, atol=0.1)
        assert_allclose(s["median"], 5.0, atol=0.1)
        assert s["std"] > 0
        assert s["ci_lower"] < s["mean"] < s["ci_upper"]
        assert s["hpd_lower"] < s["mean"] < s["hpd_upper"]
        assert s["n_samples"] == 5000
        assert "q0.025" in s
        assert "q0.975" in s


class TestMultiParamSummary:
    def test_basic(self):
        rng = np.random.default_rng(42)
        chains = {
            "mu": rng.normal(0, 1, size=(4, 2000)),
            "sigma": rng.gamma(2, 1, size=(4, 2000)),
        }
        summaries = multi_param_summary(chains)
        assert "mu" in summaries
        assert "sigma" in summaries
        assert_allclose(summaries["mu"]["mean"], 0.0, atol=0.1)


class TestFormatSummaryTable:
    def test_basic(self):
        summaries = {
            "mu": {
                "mean": 0.5,
                "std": 0.1,
                "ci_lower": 0.3,
                "ci_upper": 0.7,
                "hpd_lower": 0.35,
                "hpd_upper": 0.65,
            }
        }
        table = format_summary_table(summaries)
        assert "mu" in table
        assert "0.5000" in table


class TestFormatTraceASCII:
    def test_basic(self):
        rng = np.random.default_rng(42)
        samples = rng.normal(0, 1, size=200)
        plot = format_trace_ascii(samples, width=40, height=10)
        assert "●" in plot
        assert "Trace" in plot


class TestFormatHistogramASCII:
    def test_basic(self):
        rng = np.random.default_rng(42)
        samples = rng.normal(0, 1, size=200)
        hist = format_histogram_ascii(samples, width=30, height=10, bins=10)
        assert "█" in hist
        assert "Posterior" in hist
