"""Tests for operating characteristics (OC) table and reporting."""

import pytest

from med_clinical_trial_sim.outcomes import BinaryOutcome, ContinuousOutcome
from med_clinical_trial_sim.designs.fixed import FixedDesign
from med_clinical_trial_sim.designs.group_sequential import GroupSequentialDesign
from med_clinical_trial_sim.simulate import run_simulation
from med_clinical_trial_sim.oc import OCTable, OCRow, _wilson_ci, build_oc_table


class TestWilsonCI:
    def test_known_50pct(self):
        lo, hi = _wilson_ci(50, 100)
        assert 0.3 < lo < 0.5
        assert 0.5 < hi < 0.7

    def test_zero_count(self):
        lo, hi = _wilson_ci(0, 100)
        assert lo == 0.0

    def test_all_ones(self):
        lo, hi = _wilson_ci(100, 100)
        assert hi >= 0.99

    def test_narrower_with_more_data(self):
        lo1, hi1 = _wilson_ci(50, 100)
        lo2, hi2 = _wilson_ci(500, 1000)
        assert (hi2 - lo2) < (hi1 - lo1)


class TestOCRow:
    def test_to_dict(self):
        row = OCRow(
            scenario="test", n_reps=100, rejection_rate=0.5,
            ci_lower=0.4, ci_upper=0.6, mean_n=200,
            mean_analyses=1.0, frac_efficacy=0.0, frac_futility=0.0
        )
        d = row.to_dict()
        assert d["scenario"] == "test"
        assert d["n_reps"] == 100
        assert "ci_95" in d


class TestOCTable:
    def test_from_single_simulation(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = FixedDesign(outcome=outcome, n_per_arm=100)
        sim = run_simulation(design, n_reps=50, seed=42)
        table = OCTable.from_simulation(sim, scenario="Binary Δ=0.2")
        assert len(table.rows) == 1
        assert table.rows[0].scenario == "Binary Δ=0.2"

    def test_format_table(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = FixedDesign(outcome=outcome, n_per_arm=100)
        sim = run_simulation(design, n_reps=50, seed=42)
        table = OCTable.from_simulation(sim, scenario="test")
        formatted = table.format_table()
        assert "Scenario" in formatted
        assert "test" in formatted

    def test_from_multiple_simulations(self):
        pairs = []
        for pt in [0.3, 0.4, 0.5]:
            outcome = BinaryOutcome(p_control=0.3, p_treatment=pt)
            design = FixedDesign(outcome=outcome, n_per_arm=100)
            sim = run_simulation(design, n_reps=50, seed=42)
            label = f"p_treat={pt}"
            pairs.append((label, sim))
        table = build_oc_table(pairs)
        assert len(table.rows) == 3

    def test_str(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = FixedDesign(outcome=outcome, n_per_arm=100)
        sim = run_simulation(design, n_reps=20, seed=42)
        table = OCTable.from_simulation(sim)
        s = str(table)
        assert len(s) > 0


class TestBuildOCTable:
    def test_with_group_sequential(self):
        outcome = ContinuousOutcome(mean_control=0, std_dev=1, mean_treatment=0.5)
        design = GroupSequentialDesign(outcome=outcome, n_per_arm=100, n_analyses=3)
        sim = run_simulation(design, n_reps=50, seed=42)
        table = OCTable.from_simulation(sim, scenario="GS Continuous")
        assert table.rows[0].mean_analyses <= 3
