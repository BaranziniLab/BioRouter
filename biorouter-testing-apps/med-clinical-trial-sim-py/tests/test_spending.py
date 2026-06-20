"""Tests for alpha-spending functions."""

import math
import pytest

from med_clinical_trial_sim.spending import (
    OBrienFleming,
    Pocock,
    LinearSpending,
    SpendingPlan,
    compute_spending_plan,
    obrien_fleming_plan,
    pocock_plan,
)


# ---------------------------------------------------------------------------
# Spending function unit tests
# ---------------------------------------------------------------------------

class TestOBrienFleming:
    def test_zero_at_zero(self):
        fn = OBrienFleming()
        assert fn.spend(0.05, 0.0) == 0.0

    def test_full_at_one(self):
        fn = OBrienFleming()
        assert abs(fn.spend(0.05, 1.0) - 0.05) < 1e-10

    def test_monotonically_increasing(self):
        fn = OBrienFleming()
        alpha = 0.05
        prev = 0.0
        for t in [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]:
            val = fn.spend(alpha, t)
            assert val >= prev, f"Not monotone at t={t}: {val} < {prev}"
            prev = val

    def test_small_early_spend(self):
        """O'Brien-Fleming should spend very little early."""
        fn = OBrienFleming()
        spend_at_20pct = fn.spend(0.05, 0.2)
        spend_at_80pct = fn.spend(0.05, 0.8)
        assert spend_at_20pct < 0.01, f"Early spend too large: {spend_at_20pct}"
        assert spend_at_80pct > spend_at_20pct

    def test_total_leq_alpha(self):
        fn = OBrienFleming()
        assert fn.spend(0.05, 1.0) <= 0.05 + 1e-10


class TestPocock:
    def test_zero_at_zero(self):
        fn = Pocock()
        assert fn.spend(0.05, 0.0) == 0.0

    def test_full_at_one(self):
        fn = Pocock()
        assert abs(fn.spend(0.05, 1.0) - 0.05) < 1e-10

    def test_monotonically_increasing(self):
        fn = Pocock()
        alpha = 0.05
        prev = 0.0
        for t in [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]:
            val = fn.spend(alpha, t)
            assert val >= prev
            prev = val

    def test_more_early_spend_than_obf(self):
        """Pocock should spend more alpha earlier than O'Brien-Fleming."""
        obf = OBrienFleming()
        poc = Pocock()
        for t in [0.2, 0.4, 0.6, 0.8]:
            assert poc.spend(0.05, t) >= obf.spend(0.05, t), \
                f"Pocock should spend >= OBF at t={t}"

    def test_total_leq_alpha(self):
        fn = Pocock()
        assert fn.spend(0.05, 1.0) <= 0.05 + 1e-10


class TestLinearSpending:
    def test_linear(self):
        fn = LinearSpending()
        for t in [0.0, 0.25, 0.5, 0.75, 1.0]:
            assert abs(fn.spend(0.05, t) - 0.05 * t) < 1e-10


# ---------------------------------------------------------------------------
# SpendingPlan tests
# ---------------------------------------------------------------------------

class TestSpendingPlan:
    def test_equally_spaced(self):
        plan = compute_spending_plan(OBrienFleming(), 0.05, 5)
        assert plan.n_analyses == 5
        assert len(plan.info_fractions) == 5
        assert len(plan.local_alphas) == 5
        assert abs(plan.info_fractions[-1] - 1.0) < 1e-10

    def test_cumulative_alphas_sum_to_total(self):
        plan = compute_spending_plan(Pocock(), 0.05, 5)
        assert abs(plan.cumulative_spends[-1] - 0.05) < 1e-10

    def test_local_alphas_nonnegative(self):
        plan = compute_spending_plan(OBrienFleming(), 0.05, 5)
        for a in plan.local_alphas:
            assert a >= 0.0

    def test_critical_values_positive(self):
        plan = compute_spending_plan(OBrienFleming(), 0.05, 5)
        for cv in plan.critical_values:
            assert cv > 0.0

    def test_custom_info_fractions(self):
        fracs = [0.25, 0.5, 0.75, 1.0]
        plan = compute_spending_plan(Pocock(), 0.05, 4, fracs)
        assert plan.info_fractions == fracs
        assert len(plan.local_alphas) == 4

    def test_obf_plan(self):
        plan = obrien_fleming_plan(0.05, 3)
        assert plan.n_analyses == 3
        # OBF should have small early local alphas
        assert plan.local_alphas[0] < plan.local_alphas[-1]

    def test_pocock_plan(self):
        plan = pocock_plan(0.05, 4)
        assert plan.n_analyses == 4

    def test_mismatched_lengths_raises(self):
        with pytest.raises(ValueError):
            compute_spending_plan(OBrienFleming(), 0.05, 3, [0.3, 0.6])


# ---------------------------------------------------------------------------
# Edge cases
# ---------------------------------------------------------------------------

class TestEdgeCases:
    def test_single_analysis(self):
        plan = compute_spending_plan(OBrienFleming(), 0.05, 1)
        # With one analysis, all alpha should be spent
        assert abs(plan.local_alphas[0] - 0.05) < 1e-10

    def test_many_analyses(self):
        plan = compute_spending_plan(Pocock(), 0.05, 20)
        assert plan.n_analyses == 20
        assert abs(plan.cumulative_spends[-1] - 0.05) < 1e-10

    def test_alpha_0025(self):
        """Common one-sided alpha for two-sided 0.05."""
        plan = compute_spending_plan(OBrienFleming(), 0.025, 5)
        assert abs(plan.cumulative_spends[-1] - 0.025) < 1e-10
