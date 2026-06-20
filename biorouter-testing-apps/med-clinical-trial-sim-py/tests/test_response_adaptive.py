"""Tests for the response-adaptive randomisation design."""

import pytest

from med_clinical_trial_sim.outcomes import BinaryOutcome, ContinuousOutcome
from med_clinical_trial_sim.designs.response_adaptive import (
    ResponseAdaptiveDesign,
    bayesian_allocation,
    thompson_allocation,
    _beta_posterior,
    _beta_mean,
    _normal_posterior,
)


# ---------------------------------------------------------------------------
# Allocation rule unit tests
# ---------------------------------------------------------------------------

class TestBayesianAllocation:
    def test_equal_when_equal_means(self):
        probs = bayesian_allocation([0.5, 0.5])
        assert abs(probs[0] - 0.5) < 0.01
        assert abs(probs[1] - 0.5) < 0.01

    def test_biased_toward_better(self):
        probs = bayesian_allocation([0.8, 0.3])
        assert probs[0] > probs[1]

    def test_sums_to_one(self):
        probs = bayesian_allocation([0.1, 0.5, 0.9])
        assert abs(sum(probs) - 1.0) < 1e-10

    def test_min_prob_floor(self):
        probs = bayesian_allocation([0.99, 0.01], min_prob=0.1)
        assert all(p >= 0.1 - 1e-10 for p in probs)

    def test_three_arms(self):
        probs = bayesian_allocation([0.2, 0.5, 0.8])
        assert len(probs) == 3
        assert abs(sum(probs) - 1.0) < 1e-10
        assert probs[2] > probs[0]


class TestPosteriorHelpers:
    def test_beta_posterior_prior_only(self):
        a, b = _beta_posterior(1.0, 1.0, 0, 0)
        assert a == 1.0
        assert b == 1.0

    def test_beta_posterior_update(self):
        a, b = _beta_posterior(1.0, 1.0, 10, 5)
        assert a == 11.0
        assert b == 6.0

    def test_beta_mean(self):
        assert abs(_beta_mean(10.0, 10.0) - 0.5) < 1e-10

    def test_normal_posterior_prior_only(self):
        mu, var = _normal_posterior(0.0, 100.0, [], 1.0)
        assert mu == 0.0
        assert var == 100.0

    def test_normal_posterior_converges(self):
        data = [1.0] * 100
        mu, var = _normal_posterior(0.0, 100.0, data, 1.0)
        assert abs(mu - 1.0) < 0.1
        assert var < 1.0


# ---------------------------------------------------------------------------
# ResponseAdaptiveDesign tests
# ---------------------------------------------------------------------------

class TestResponseAdaptiveDesign:
    def test_binary_basic(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = ResponseAdaptiveDesign(outcome=outcome, n_max=100)
        data = design.generate_data(42)
        assert "ctrl" in data
        assert "treat" in data
        assert data["n_ctrl"] + data["n_treat"] <= 100
        assert "alloc_probs" in data

    def test_continuous_basic(self):
        outcome = ContinuousOutcome(mean_control=0, std_dev=1, mean_treatment=0.5)
        design = ResponseAdaptiveDesign(outcome=outcome, n_max=100)
        data = design.generate_data(42)
        assert data["n_ctrl"] + data["n_treat"] <= 100

    def test_allocation_biases_toward_better_arm(self):
        """With a strong treatment effect, more patients should be allocated to treatment."""
        outcome = BinaryOutcome(p_control=0.1, p_treatment=0.8)
        design = ResponseAdaptiveDesign(
            outcome=outcome, n_max=200, block_size=10, allocation="bayesian"
        )
        data = design.generate_data(42)
        # Treatment arm should have more patients
        assert data["n_treat"] > data["n_ctrl"], \
            f"Expected more treated: treat={data['n_treat']}, ctrl={data['n_ctrl']}"

    def test_equal_allocation_under_null(self):
        """Under the null, allocation should be roughly balanced."""
        outcome = BinaryOutcome(p_control=0.5, p_treatment=0.5)
        design = ResponseAdaptiveDesign(
            outcome=outcome, n_max=200, block_size=10, allocation="bayesian"
        )
        ratios = []
        for seed in range(50):
            data = design.generate_data(seed)
            n0, n1 = data["n_ctrl"], data["n_treat"]
            if n0 + n1 > 0:
                ratios.append(n1 / (n0 + n1))
        mean_ratio = sum(ratios) / len(ratios)
        # Should be close to 0.5
        assert 0.35 < mean_ratio < 0.65, f"Expected balanced allocation, got mean ratio={mean_ratio}"

    def test_max_sample_size_not_exceeded(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = ResponseAdaptiveDesign(outcome=outcome, n_max=80)
        for seed in range(50):
            data = design.generate_data(seed)
            assert data["n_ctrl"] + data["n_treat"] <= 80

    def test_efficacy_stopping(self):
        """With efficacy_bound set, early stopping should sometimes occur."""
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.8)  # Huge effect
        design = ResponseAdaptiveDesign(
            outcome=outcome, n_max=300, block_size=10, efficacy_bound=2.0
        )
        early = 0
        for seed in range(100):
            data = design.generate_data(seed)
            if data["stopped_early"]:
                early += 1
        # With a huge effect and large max N, some should stop early
        assert early > 0, "Expected at least some early stopping"

    def test_type_i_error(self):
        """Under the null, rejection rate should be ~alpha."""
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.3)
        design = ResponseAdaptiveDesign(outcome=outcome, n_max=200, alpha=0.05)
        rejections = sum(
            1 for seed in range(500)
            if design.generate_data(seed)["reject"]
        )
        rate = rejections / 500
        assert rate < 0.12, f"Type-I error too high: {rate}"

    def test_repr(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = ResponseAdaptiveDesign(outcome=outcome, n_max=100)
        r = repr(design)
        assert "ResponseAdaptiveDesign" in r
