"""Tests for the fixed sample-size design."""

import pytest

from med_clinical_trial_sim.outcomes import BinaryOutcome, ContinuousOutcome, TimeToEventOutcome
from med_clinical_trial_sim.designs.fixed import (
    FixedDesign,
    _ss_binary,
    _ss_continuous,
    _ss_tte,
)


# ---------------------------------------------------------------------------
# Sample-size formula tests
# ---------------------------------------------------------------------------

class TestSSBinary:
    def test_known_value(self):
        # Two proportions 0.3 vs 0.5, alpha=0.05, power=0.8
        n = _ss_binary(0.3, 0.5, 0.05, 0.8)
        # Standard formula gives ~87 per arm
        assert 70 < n < 120, f"Unexpected n={n}"

    def test_larger_effect_smaller_n(self):
        n_small = _ss_binary(0.3, 0.7, 0.05, 0.8)
        n_large = _ss_binary(0.3, 0.4, 0.05, 0.8)
        assert n_small < n_large

    def test_higher_power_larger_n(self):
        n80 = _ss_binary(0.3, 0.5, 0.05, 0.80)
        n90 = _ss_binary(0.3, 0.5, 0.05, 0.90)
        assert n90 > n80

    def test_at_least_1(self):
        # Even with huge effect
        n = _ss_binary(0.01, 0.99, 0.05, 0.99)
        assert n >= 1


class TestSSContinuous:
    def test_known_value(self):
        n = _ss_continuous(0, 0.5, 1.0, 0.05, 0.8)
        # Standard: n ≈ 64
        assert 40 < n < 100

    def test_larger_effect_smaller_n(self):
        n_small = _ss_continuous(0, 1.0, 1.0, 0.05, 0.8)
        n_large = _ss_continuous(0, 0.3, 1.0, 0.05, 0.8)
        assert n_small < n_large


class TestSSTTE:
    def test_known_value(self):
        n = _ss_tte(12, 0.65, 0.05, 0.8, events_frac=0.8)
        # Typical: ~100-200 per arm
        assert 50 < n < 400

    def test_hr_1_needs_infinite_sample(self):
        # HR=1 means no effect — n should be very large
        n = _ss_tte(12, 1.0, 0.05, 0.8)
        assert n > 1000


# ---------------------------------------------------------------------------
# FixedDesign tests
# ---------------------------------------------------------------------------

class TestFixedDesign:
    def test_binary_auto_n(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = FixedDesign(outcome=outcome, alpha=0.05, power=0.80)
        assert design.n_per_arm > 0
        assert design.total_sample_size == design.n_per_arm * 2

    def test_continuous_auto_n(self):
        outcome = ContinuousOutcome(mean_control=0, std_dev=1, mean_treatment=0.5)
        design = FixedDesign(outcome=outcome, alpha=0.05, power=0.80)
        assert design.n_per_arm > 0

    def test_explicit_n(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = FixedDesign(outcome=outcome, n_per_arm=50)
        assert design.n_per_arm == 50

    def test_dropout_increases_n(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        d1 = FixedDesign(outcome=outcome, alpha=0.05, power=0.80, dropout_rate=0.0)
        d2 = FixedDesign(outcome=outcome, alpha=0.05, power=0.80, dropout_rate=0.2)
        assert d2.n_per_arm >= d1.n_per_arm

    def test_generate_data_keys(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = FixedDesign(outcome=outcome, n_per_arm=50)
        data = design.generate_data(42)
        assert "ctrl" in data
        assert "treat" in data
        assert "z" in data
        assert "p_value" in data
        assert "reject" in data
        assert data["n_ctrl"] == 50
        assert data["n_treat"] == 50
        assert data["n_analyses"] == 1
        assert data["stopped_early"] is False

    def test_under_null_low_rejection(self):
        """Type-I error for fixed design should be ~alpha."""
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.3)  # Null
        design = FixedDesign(outcome=outcome, n_per_arm=200, alpha=0.05)
        rejections = sum(
            1 for seed in range(1000)
            if design.generate_data(seed)["reject"]
        )
        rate = rejections / 1000
        assert rate < 0.10, f"Type-I error too high: {rate}"
        assert rate > 0.01, f"Type-I error too low: {rate}"

    def test_under_effect_high_power(self):
        """Power for fixed design should be > 0.80 with n=200 and d=0.5."""
        outcome = ContinuousOutcome(mean_control=0, std_dev=1, mean_treatment=0.5)
        design = FixedDesign(outcome=outcome, n_per_arm=200, alpha=0.05)
        rejections = sum(
            1 for seed in range(500)
            if design.generate_data(seed)["reject"]
        )
        power = rejections / 500
        assert power > 0.85, f"Power too low: {power}"

    def test_repr(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = FixedDesign(outcome=outcome, n_per_arm=100)
        r = repr(design)
        assert "FixedDesign" in r
        assert "n_per_arm=100" in r
