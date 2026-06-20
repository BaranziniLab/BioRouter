"""Tests for the CLI module."""

import pytest

from med_clinical_trial_sim.cli import build_parser, main, _make_outcome, _make_design
from med_clinical_trial_sim.outcomes import BinaryOutcome, ContinuousOutcome, TimeToEventOutcome
from med_clinical_trial_sim.designs.fixed import FixedDesign
from med_clinical_trial_sim.designs.group_sequential import GroupSequentialDesign
from med_clinical_trial_sim.designs.response_adaptive import ResponseAdaptiveDesign


class TestBuildParser:
    def test_defaults(self):
        parser = build_parser()
        args = parser.parse_args([])
        assert args.design == "fixed"
        assert args.outcome == "binary"
        assert args.alpha == 0.05
        assert args.n_reps == 1000

    def test_custom_args(self):
        parser = build_parser()
        args = parser.parse_args([
            "--design", "group_sequential",
            "--outcome", "continuous",
            "--n-analyses", "5",
            "--spending", "pocock",
            "--n-reps", "500",
        ])
        assert args.design == "group_sequential"
        assert args.outcome == "continuous"
        assert args.n_analyses == 5
        assert args.spending == "pocock"
        assert args.n_reps == 500


class TestMakeOutcome:
    def test_binary(self):
        args = build_parser().parse_args([
            "--outcome", "binary",
            "--p-control", "0.2", "--p-treatment", "0.6",
        ])
        m = _make_outcome(args)
        assert isinstance(m, BinaryOutcome)
        assert m.p_control == 0.2
        assert m.p_treatment == 0.6

    def test_continuous(self):
        args = build_parser().parse_args([
            "--outcome", "continuous",
            "--mean-control", "1.0", "--mean-treatment", "2.0", "--std-dev", "0.5",
        ])
        m = _make_outcome(args)
        assert isinstance(m, ContinuousOutcome)
        assert m.mean_control == 1.0

    def test_tte(self):
        args = build_parser().parse_args([
            "--outcome", "tte",
            "--median-control", "10", "--hazard-ratio", "0.5",
        ])
        m = _make_outcome(args)
        assert isinstance(m, TimeToEventOutcome)


class TestMakeDesign:
    def test_fixed(self):
        args = build_parser().parse_args([
            "--design", "fixed", "--outcome", "binary",
            "--n-per-arm", "100",
        ])
        d = _make_design(args)
        assert isinstance(d, FixedDesign)
        assert d.n_per_arm == 100

    def test_group_sequential(self):
        args = build_parser().parse_args([
            "--design", "group_sequential", "--outcome", "binary",
            "--n-per-arm", "100", "--n-analyses", "4",
        ])
        d = _make_design(args)
        assert isinstance(d, GroupSequentialDesign)
        assert d.n_analyses == 4

    def test_response_adaptive(self):
        args = build_parser().parse_args([
            "--design", "response_adaptive", "--outcome", "binary",
            "--n-max", "150",
        ])
        d = _make_design(args)
        assert isinstance(d, ResponseAdaptiveDesign)
        assert d.n_max == 150


class TestMainIntegration:
    def test_fixed_runs(self, capsys):
        """CLI runs to completion with a fixed design."""
        ret = main(["--design", "fixed", "--outcome", "binary",
                     "--n-per-arm", "30", "--n-reps", "50"])
        assert ret == 0
        captured = capsys.readouterr()
        assert "Operating Characteristics" in captured.out

    def test_group_sequential_runs(self, capsys):
        """CLI runs with a group-sequential design."""
        ret = main(["--design", "group_sequential", "--outcome", "binary",
                     "--n-per-arm", "50", "--n-analyses", "3", "--n-reps", "30"])
        assert ret == 0

    def test_response_adaptive_runs(self, capsys):
        """CLI runs with a response-adaptive design."""
        ret = main(["--design", "response_adaptive", "--outcome", "binary",
                     "--n-max", "60", "--n-reps", "30"])
        assert ret == 0

    def test_continuous_runs(self, capsys):
        """CLI runs with a continuous endpoint."""
        ret = main(["--design", "fixed", "--outcome", "continuous",
                     "--n-per-arm", "30", "--n-reps", "30"])
        assert ret == 0

    def test_sweep_effect(self, capsys):
        """Effect-size sweep produces a multi-row OC table."""
        ret = main(["--design", "fixed", "--outcome", "binary",
                     "--n-per-arm", "30", "--n-reps", "30", "--sweep-effect"])
        assert ret == 0
        captured = capsys.readouterr()
        # Should have multiple rows
        assert "p_ctrl" in captured.out or "p_treat" in captured.out
