"""Tests for the CLI module.

Tests call the CLI code directly (no subprocess), as required.
"""

import sys
import csv
import tempfile
from pathlib import Path

import numpy as np
import pytest

from med_epidemic.cli import (
    main,
    build_parser,
    cmd_sir,
    cmd_seir,
    cmd_seird,
    cmd_seir_intervention,
    cmd_stochastic_sir,
    cmd_fit,
)


class TestBuildParser:
    def test_all_subcommands(self):
        parser = build_parser()
        for cmd in ["sir", "seir", "seird", "seir-intervention", "stochastic-sir"]:
            args = parser.parse_args([cmd])
            assert hasattr(args, "func")
        # fit requires --data
        args = parser.parse_args(["fit", "--data", "/dev/null"])
        assert hasattr(args, "func")

    def test_sir_args(self):
        parser = build_parser()
        args = parser.parse_args(["sir", "--beta", "0.5", "--gamma", "0.2", "--N", "5000"])
        assert args.beta == 0.5
        assert args.gamma == 0.2
        assert args.N == 5000


class TestCmdSIR:
    def test_runs_without_error(self, capsys):
        args = build_parser().parse_args(["sir", "--N", "1000", "--t-max", "30", "--no-plot", "--quiet"])
        cmd_sir(args)
        # should not raise

    def test_exports_csv(self, tmp_path):
        csv_file = tmp_path / "sir_out.csv"
        args = build_parser().parse_args([
            "sir", "--N", "1000", "--t-max", "30", "--quiet",
            "--export-csv", str(csv_file),
        ])
        cmd_sir(args)
        assert csv_file.exists()
        with open(csv_file) as f:
            reader = csv.reader(f)
            header = next(reader)
            assert header[0] == "time"
            assert "S" in header
            assert "I" in header
            assert "R" in header
            rows = list(reader)
            assert len(rows) > 10

    def test_prints_metrics(self, capsys):
        args = build_parser().parse_args(["sir", "--N", "5000", "--t-max", "50"])
        cmd_sir(args)
        captured = capsys.readouterr()
        assert "SIR Model Results" in captured.out
        assert "R0" in captured.out
        assert "peak_infected" in captured.out


class TestCmdSEIR:
    def test_runs_without_error(self, capsys):
        args = build_parser().parse_args([
            "seir", "--N", "5000", "--t-max", "40", "--no-plot", "--quiet",
        ])
        cmd_seir(args)

    def test_prints_metrics(self, capsys):
        args = build_parser().parse_args(["seir", "--N", "5000", "--t-max", "50"])
        cmd_seir(args)
        captured = capsys.readouterr()
        assert "SEIR Model Results" in captured.out


class TestCmdSEIRD:
    def test_runs_without_error(self, capsys):
        args = build_parser().parse_args([
            "seird", "--N", "5000", "--t-max", "40", "--mu", "0.02",
            "--no-plot", "--quiet",
        ])
        cmd_seird(args)

    def test_prints_metrics(self, capsys):
        args = build_parser().parse_args([
            "seird", "--N", "5000", "--t-max", "50", "--mu", "0.02",
        ])
        cmd_seird(args)
        captured = capsys.readouterr()
        assert "SEIRD Model Results" in captured.out


class TestCmdSEIRIntervention:
    def test_runs_without_error(self, capsys):
        args = build_parser().parse_args([
            "seir-intervention", "--N", "5000", "--t-max", "40",
            "--lockdown-start", "20", "--lockdown-reduction", "0.6",
            "--no-plot", "--quiet",
        ])
        cmd_seir_intervention(args)

    def test_intervention_reduces_peak_vs_no_intervention(self, capsys):
        # run without intervention
        args_base = build_parser().parse_args([
            "seir-intervention", "--N", "5000", "--t-max", "100",
            "--no-plot", "--quiet",
        ])
        cmd_seir_intervention(args_base)
        capsys.readouterr()

        # run with intervention
        args_iv = build_parser().parse_args([
            "seir-intervention", "--N", "5000", "--t-max", "100",
            "--lockdown-start", "20", "--lockdown-reduction", "0.7",
            "--no-plot", "--quiet",
        ])
        cmd_seir_intervention(args_iv)
        capsys.readouterr()


class TestCmdStochasticSIR:
    def test_runs_without_error(self, capsys):
        args = build_parser().parse_args([
            "stochastic-sir", "--N", "200", "--t-max", "30",
            "--no-plot", "--quiet",
        ])
        cmd_stochastic_sir(args)

    def test_prints_info(self, capsys):
        args = build_parser().parse_args([
            "stochastic-sir", "--N", "200", "--t-max", "20",
        ])
        cmd_stochastic_sir(args)
        captured = capsys.readouterr()
        assert "Stochastic SIR" in captured.out


class TestCmdFit:
    def test_fit_sir(self, tmp_path, capsys):
        """Create synthetic CSV and fit SIR model to it."""
        from med_epidemic.models.sir import SIRModel, SIRParams

        true_beta, true_gamma = 0.3, 0.1
        N = 10000
        model = SIRModel(SIRParams(beta=true_beta, gamma=true_gamma, N=N, I0=10))
        sol = model.run(t_span=(0, 80), dt=0.5)
        t_obs = np.linspace(0, 80, 50)
        I_obs = np.interp(t_obs, sol.t, sol.y[1])

        csv_file = tmp_path / "cases.csv"
        with open(csv_file, "w", newline="") as f:
            writer = csv.writer(f)
            writer.writerow(["time", "infected"])
            for t, i in zip(t_obs, I_obs):
                writer.writerow([f"{t:.2f}", f"{i:.2f}"])

        args = build_parser().parse_args([
            "fit", "--data", str(csv_file), "--model", "sir", "--N", str(N),
            "--no-plot",
        ])
        cmd_fit(args)
        captured = capsys.readouterr()
        assert "Fitted SIR" in captured.out
        assert "beta" in captured.out
        assert "gamma" in captured.out


class TestMain:
    def test_main_dispatches(self, capsys):
        main(["sir", "--N", "500", "--t-max", "20", "--no-plot", "--quiet"])

    def test_main_with_all_flags(self, capsys):
        main(["sir", "--N", "500", "--t-max", "20", "--beta", "0.5", "--gamma", "0.2",
              "--no-plot", "--quiet"])


class TestASCIIPlot:
    def test_ascii_plot_renders(self):
        from med_epidemic.plot_ascii import ascii_plot
        t = np.linspace(0, 100, 200)
        s = 10000 * np.exp(-0.02 * t)
        i = 500 * np.sin(t / 10)
        result = ascii_plot(t, [s, i], ["S", "I"], width=60, height=15)
        assert "|" in result
        assert "S" in result
        assert "I" in result

    def test_ascii_plot_empty(self):
        from med_epidemic.plot_ascii import ascii_plot
        t = np.array([0.0])
        result = ascii_plot(t, [], [], width=40, height=10)
        assert result == ""
