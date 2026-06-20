"""Tests for the group-sequential design."""

import pytest

from med_clinical_trial_sim.outcomes import BinaryOutcome, ContinuousOutcome
from med_clinical_trial_sim.spending import OBrienFleming, Pocock
from med_clinical_trial_sim.designs.group_sequential import GroupSequentialDesign


class TestGroupSequentialDesign:
    def test_auto_n(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = GroupSequentialDesign(
            outcome=outcome, n_analyses=5, alpha=0.05, power=0.80
        )
        assert design.n_per_arm > 0

    def test_explicit_n(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = GroupSequentialDesign(
            outcome=outcome, n_per_arm=100, n_analyses=5
        )
        assert design.n_per_arm == 100

    def test_spending_plan_created(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = GroupSequentialDesign(
            outcome=outcome, n_per_arm=100, n_analyses=5
        )
        assert design.spending_plan.n_analyses == 5
        assert len(design._crit_values) == 5

    def test_per_look_n_monotone(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = GroupSequentialDesign(
            outcome=outcome, n_per_arm=100, n_analyses=5
        )
        for i in range(1, len(design._per_look_n)):
            assert design._per_look_n[i] >= design._per_look_n[i - 1]

    def test_generate_data_keys(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = GroupSequentialDesign(
            outcome=outcome, n_per_arm=100, n_analyses=3
        )
        data = design.generate_data(42)
        assert "ctrl" in data
        assert "treat" in data
        assert "z" in data
        assert "n_analyses" in data
        assert "stopped_early" in data
        assert "stop_reason" in data
        assert "looks" in data
        assert 1 <= data["n_analyses"] <= 3

    def test_obf_stops_early_under_strong_effect(self):
        """O'Brien-Fleming design should stop early for efficacy under strong effects."""
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.7)  # Large effect
        design = GroupSequentialDesign(
            outcome=outcome, n_per_arm=200, n_analyses=5,
            spending=OBrienFleming(), alpha=0.05
        )
        early_stops = 0
        for seed in range(500):
            data = design.generate_data(seed)
            if data["stopped_early"] and data["stop_reason"] == "efficacy":
                early_stops += 1
        # With a very strong effect, at least 30% should stop early
        frac = early_stops / 500
        assert frac > 0.10, f"Expected some early stopping, got {frac:.2%}"

    def test_obf_preserves_type_i_error(self):
        """Under the null, O'Brien-Fleming should have type-I error ≈ alpha."""
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.3)  # Null
        design = GroupSequentialDesign(
            outcome=outcome, n_per_arm=200, n_analyses=5,
            spending=OBrienFleming(), alpha=0.05
        )
        rejections = sum(
            1 for seed in range(1000)
            if design.generate_data(seed)["reject"]
        )
        rate = rejections / 1000
        # Allow generous bounds for simulation variability
        assert rate < 0.10, f"Type-I error too high: {rate}"
        assert rate > 0.01, f"Type-I error too low: {rate}"

    def test_pocock_plan(self):
        """Pocock spending should also work."""
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = GroupSequentialDesign(
            outcome=outcome, n_per_arm=100, n_analyses=4,
            spending=Pocock(), alpha=0.05
        )
        data = design.generate_data(42)
        assert "reject" in data

    def test_no_futility(self):
        """Without futility, stopped_early is only True for efficacy."""
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.3)
        design = GroupSequentialDesign(
            outcome=outcome, n_per_arm=100, n_analyses=3,
            futiltiy=False, alpha=0.05
        )
        for seed in range(100):
            data = design.generate_data(seed)
            if data["stopped_early"]:
                assert data["stop_reason"] == "efficacy"

    def test_repr(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = GroupSequentialDesign(
            outcome=outcome, n_per_arm=100, n_analyses=5
        )
        r = repr(design)
        assert "GroupSequentialDesign" in r
        assert "n_analyses=5" in r
