"""Tests for the simulation engine."""

import pytest

from med_clinical_trial_sim.outcomes import BinaryOutcome, ContinuousOutcome
from med_clinical_trial_sim.designs.fixed import FixedDesign
from med_clinical_trial_sim.designs.group_sequential import GroupSequentialDesign
from med_clinical_trial_sim.simulate import run_simulation, SimulationOutput, SimResult


class TestSimResult:
    def test_total_n(self):
        sr = SimResult(reject=False, n_ctrl=50, n_treat=50, n_analyses=1,
                       stopped_early=False, stop_reason=None, z=0.5, p_value=0.6)
        assert sr.total_n == 100

    def test_total_n_explicit(self):
        sr = SimResult(reject=True, n_ctrl=30, n_treat=40, n_analyses=3,
                       stopped_early=True, stop_reason="efficacy", z=2.5, p_value=0.01,
                       total_n=80)
        assert sr.total_n == 80


class TestSimulationOutput:
    def test_rejections(self):
        results = [
            SimResult(reject=True, n_ctrl=50, n_treat=50, n_analyses=1,
                      stopped_early=False, stop_reason=None, z=2.0, p_value=0.04),
            SimResult(reject=False, n_ctrl=50, n_treat=50, n_analyses=1,
                      stopped_early=False, stop_reason=None, z=0.5, p_value=0.6),
            SimResult(reject=True, n_ctrl=50, n_treat=50, n_analyses=1,
                      stopped_early=False, stop_reason=None, z=2.5, p_value=0.01),
        ]
        sim = SimulationOutput(design=None, n_reps=3, seed=42, results=results)
        assert sim.rejections == 2
        assert abs(sim.rejections_rate - 2 / 3) < 1e-10

    def test_summary(self):
        results = [
            SimResult(reject=True, n_ctrl=50, n_treat=50, n_analyses=1,
                      stopped_early=False, stop_reason=None, z=2.0, p_value=0.04),
        ]
        sim = SimulationOutput(design="test", n_reps=1, seed=42, results=results)
        s = sim.summary()
        assert "n_reps" in s
        assert s["n_reps"] == 1


class TestRunSimulation:
    def test_fixed_returns_output(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = FixedDesign(outcome=outcome, n_per_arm=50)
        sim = run_simulation(design, n_reps=50, seed=42)
        assert isinstance(sim, SimulationOutput)
        assert sim.n_reps == 50
        assert len(sim.results) == 50

    def test_group_sequential_returns_output(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = GroupSequentialDesign(outcome=outcome, n_per_arm=100, n_analyses=3)
        sim = run_simulation(design, n_reps=50, seed=42)
        assert sim.n_reps == 50

    def test_seed_reproducibility(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = FixedDesign(outcome=outcome, n_per_arm=50)
        sim1 = run_simulation(design, n_reps=100, seed=123)
        sim2 = run_simulation(design, n_reps=100, seed=123)
        assert sim1.rejections == sim2.rejections
        for r1, r2 in zip(sim1.results, sim2.results):
            assert r1.z == r2.z

    def test_different_seeds_give_different_results(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = FixedDesign(outcome=outcome, n_per_arm=50)
        sim1 = run_simulation(design, n_reps=100, seed=1)
        sim2 = run_simulation(design, n_reps=100, seed=2)
        # At least one result should differ
        assert any(r1.z != r2.z for r1, r2 in zip(sim1.results, sim2.results))

    def test_elapsed_time_positive(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = FixedDesign(outcome=outcome, n_per_arm=50)
        sim = run_simulation(design, n_reps=20, seed=42)
        assert sim.elapsed_sec >= 0.0

    def test_mean_sample_size(self):
        outcome = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        design = FixedDesign(outcome=outcome, n_per_arm=100)
        sim = run_simulation(design, n_reps=50, seed=42)
        assert sim.mean_sample_size == 200.0  # fixed design always uses n_per_arm * 2
