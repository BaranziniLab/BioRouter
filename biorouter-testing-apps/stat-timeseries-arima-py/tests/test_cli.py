"""Tests for tskit.cli — command-line interface."""

import csv
import os
import tempfile
from tskit.cli import read_csv, ascii_plot, main


def _make_csv(values, path):
    """Create a simple CSV file with a 'value' column."""
    with open(path, "w", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(["time", "value"])
        for i, v in enumerate(values):
            writer.writerow([i, v])
    return path


class TestReadCSV:
    def test_read_csv(self):
        tmpdir = tempfile.mkdtemp()
        path = os.path.join(tmpdir, "test.csv")
        _make_csv([1.0, 2.0, 3.0], path)
        data = read_csv(path, "value")
        assert data == [1.0, 2.0, 3.0]


class TestASCIIPlot:
    def test_ascii_plot_returns_string(self):
        result = ascii_plot([1, 2, 3, 4, 5], width=20, height=10)
        assert isinstance(result, str)
        assert "|" in result

    def test_ascii_plot_empty(self):
        result = ascii_plot([])
        assert "empty" in result.lower()


class TestCLIMain:
    def test_cli_arima(self):
        """Run CLI with ARIMA model on test data."""
        tmpdir = tempfile.mkdtemp()
        path = os.path.join(tmpdir, "data.csv")
        _make_csv([i * 0.5 + (i % 3) * 0.1 for i in range(100)], path)
        # Should run without error
        main([path, "--model", "arima", "--p", "1", "--d", "1", "--q", "0", "--h", "5"])

    def test_cli_holtwinters(self):
        """Run CLI with Holt-Winters."""
        tmpdir = tempfile.mkdtemp()
        path = os.path.join(tmpdir, "data.csv")
        import math
        vals = [100 + 5 * math.sin(2 * math.pi * i / 12) + 0.1 * i for i in range(60)]
        _make_csv(vals, path)
        main([path, "--model", "holtwinters", "--m", "12", "--h", "12"])

    def test_cli_auto(self):
        """Run CLI with auto order selection."""
        tmpdir = tempfile.mkdtemp()
        path = os.path.join(tmpdir, "data.csv")
        from tskit.numerics import set_seed, simulate_ar
        set_seed(42)
        x = simulate_ar([0.5], n=200)
        _make_csv(x, path)
        main([path, "--model", "arima", "--auto", "--h", "5"])

    def test_cli_backtest(self):
        """Run CLI with backtest flag."""
        tmpdir = tempfile.mkdtemp()
        path = os.path.join(tmpdir, "data.csv")
        from tskit.numerics import set_seed, simulate_ar
        set_seed(42)
        x = simulate_ar([0.5], n=300)
        _make_csv(x, path)
        main([path, "--model", "arima", "--p", "1", "--d", "0", "--q", "0",
              "--h", "5", "--backtest", "--min-train", "100"])
