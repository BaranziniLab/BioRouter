"""Tests for CLI driver."""

import sys
import pytest
import numpy as np

from bayesmcmc.cli import parse_args, main


class TestParseArgs:
    def test_defaults(self):
        args = parse_args([])
        assert args.model == "beta_binomial"
        assert args.n_samples == 5000
        assert args.n_chains == 4
        assert args.seed == 42
        assert args.sampler == "mh"

    def test_custom_data(self):
        args = parse_args(["--data", "1,1,1,0,0"])
        assert args.data == "1,1,1,0,0"

    def test_model_choice(self):
        args = parse_args(["--model", "linear_regression"])
        assert args.model == "linear_regression"

    def test_sampler_choice(self):
        args = parse_args(["--sampler", "gibbs"])
        assert args.sampler == "gibbs"

    def test_quiet(self):
        args = parse_args(["--quiet"])
        assert args.quiet is True


class TestCLIIntegration:
    def test_beta_binomial_runs(self):
        """CLI beta_binomial should run without error."""
        main(["--model", "beta_binomial", "--data", "1,1,1,0,0",
              "--n-samples", "2000", "--n-chains", "2", "--burn-in", "500",
              "--quiet"])

    def test_linear_regression_runs(self):
        """CLI linear_regression should run without error."""
        main(["--model", "linear_regression",
              "--n-samples", "2000", "--n-chains", "2", "--burn-in", "500",
              "--quiet"])

    def test_hierarchical_runs(self):
        """CLI hierarchical_normal should run without error."""
        main(["--model", "hierarchical_normal",
              "--n-samples", "2000", "--n-chains", "2", "--burn-in", "500",
              "--quiet"])

    def test_gibbs_sampler(self):
        """CLI with Gibbs sampler should run without error."""
        main(["--model", "beta_binomial", "--data", "1,1,1,0,0",
              "--sampler", "gibbs",
              "--n-samples", "2000", "--n-chains", "2", "--burn-in", "500",
              "--quiet"])

    def test_slice_sampler(self):
        """CLI with Slice sampler should run without error."""
        main(["--model", "beta_binomial", "--data", "1,1,1,0,0",
              "--sampler", "slice",
              "--n-samples", "2000", "--n-chains", "2", "--burn-in", "500",
              "--quiet"])

    def test_with_ascii_output(self):
        """CLI should produce ASCII output by default."""
        main(["--model", "beta_binomial", "--data", "1,1,1,0,0",
              "--n-samples", "1000", "--n-chains", "2", "--burn-in", "300"])
