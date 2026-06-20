"""CLI driver — fit models, print forecasts, ASCII plots.

Usage
-----
    python -m tskit.cli data.csv [--model arima] [--p 2] [--d 1] [--q 1]
                                 [--h 10] [--seasonal] [--m 12]
                                 [--auto] [--backtest] [--plot]
"""

from __future__ import annotations

import argparse
import csv
import math
import sys
from pathlib import Path
from typing import List

from .acf import acf as acf_fn, pacf as pacf_fn, adf_test
from .arima import fit_arima, forecast_arima
from .sarima import fit_sarima, forecast_sarima
from .holtwinters import fit_holt_winters, forecast_hw
from .autoorder import auto_arima, auto_sarima
from .backtest import rolling_backtest, evaluate_forecast
from .numerics import to_vec


def read_csv(path: str, column: str | None = None) -> List[float]:
    """Read a time series from a CSV file."""
    with open(path, "r") as f:
        reader = csv.DictReader(f)
        headers = reader.fieldnames or []
        if column and column in headers:
            return [float(row[column]) for row in reader]
        # Try first numeric column
        for h in headers:
            try:
                return [float(row[h]) for row in csv.DictReader(open(path))]
            except (ValueError, KeyError):
                continue
        raise ValueError(f"Cannot parse numeric data from {path}")


def ascii_plot(values: List[float], width: int = 60, height: int = 20, title: str = "") -> str:
    """Render a simple ASCII time-series plot."""
    if not values:
        return "(empty series)"
    mn = min(values)
    mx = max(values)
    rng = mx - mn if mx != mn else 1.0
    lines = []
    if title:
        lines.append(f"  {title}")
    lines.append(f"  {mx:>10.2f} |")
    for row in range(height - 1, -1, -1):
        threshold = mn + (row / (height - 1)) * rng
        bar = ""
        # Sample values to fit width
        step = max(1, len(values) // width)
        for i in range(0, min(len(values), width * step), step):
            v = values[i]
            if v >= threshold:
                bar += "█"
            else:
                bar += " "
        lines.append(f"  {threshold:>10.2f} |{bar}")
    lines.append(f"  {mn:>10.2f} +" + "─" * min(len(values), width))
    return "\n".join(lines)


def main(argv: List[str] | None = None):
    parser = argparse.ArgumentParser(description="tskit — time-series forecasting CLI")
    parser.add_argument("csv_file", help="Path to CSV file with time series")
    parser.add_argument("--column", help="CSV column name to use")
    parser.add_argument("--model", choices=["arima", "sarima", "holtwinters"], default="arima")
    parser.add_argument("--p", type=int, default=1, help="AR order")
    parser.add_argument("--d", type=int, default=1, help="Differencing order")
    parser.add_argument("--q", type=int, default=1, help="MA order")
    parser.add_argument("--h", type=int, default=10, help="Forecast horizon")
    parser.add_argument("--seasonal", action="store_true", help="Use seasonal model")
    parser.add_argument("--m", type=int, default=12, help="Seasonal period")
    parser.add_argument("--P", type=int, default=1, help="Seasonal AR order")
    parser.add_argument("--D", type=int, default=1, help="Seasonal differencing")
    parser.add_argument("--Q", type=int, default=1, help="Seasonal MA order")
    parser.add_argument("--auto", action="store_true", help="Automatic order selection")
    parser.add_argument("--backtest", action="store_true", help="Run rolling backtest")
    parser.add_argument("--plot", action="store_true", help="Show ASCII plot")
    parser.add_argument("--nlags", type=int, default=30, help="ACF/PACF lags to display")
    parser.add_argument("--min-train", type=int, default=None, help="Minimum training window for backtest")
    args = parser.parse_args(argv)

    # Read data
    series = read_csv(args.csv_file, args.column)
    print(f"\nLoaded {len(series)} observations from {args.csv_file}")
    print(f"  Range: [{min(series):.2f}, {max(series):.2f}]")

    # Stationarity test
    result = adf_test(series)
    print(f"\nADF test: statistic={result['statistic']:.3f}, p≈{result['p_value']:.3f}")
    if result["reject_5pct"]:
        print("  → Series appears stationary (reject unit root at 5%)")
    else:
        print("  → Series may be non-stationary (fail to reject unit root)")

    # ACF / PACF
    acf_vals = acf_fn(series, args.nlags)
    pacf_vals = pacf_fn(series, args.nlags)
    print(f"\nACF  (first {min(10, args.nlags)} lags): {[f'{v:.3f}' for v in acf_vals[:11]]}")
    print(f"PACF (first {min(10, args.nlags)} lags): {[f'{v:.3f}' for v in pacf_vals[:11]]}")

    # Fit model
    h = args.h
    if args.model == "arima":
        if args.auto:
            print(f"\nAuto ARIMA search (max_p=5, max_d=2, max_q=5)...")
            selection = auto_arima(series, max_p=5, max_d=2, max_q=5)
            p, d, q = selection["order"]
            model = selection["model"]
            print(f"  Best: ARIMA({p},{d},{q})  {selection['criterion'].upper()}={selection['score']:.2f}")
        else:
            p, d, q = args.p, args.d, args.q
            print(f"\nFitting ARIMA({p},{d},{q})...")
            model = fit_arima(series, p, d, q)
        forecast = forecast_arima(model, h)
    elif args.model == "sarima":
        if args.auto:
            print(f"\nAuto SARIMA search...")
            selection = auto_sarima(series, m=args.m)
            p, d, q, P, D, Q = selection["order"]
            model = selection["model"]
            print(f"  Best: SARIMA({p},{d},{q})x({P},{D},{Q})_{args.m}  {selection['criterion'].upper()}={selection['score']:.2f}")
        else:
            p, d, q, P, D, Q = args.p, args.d, args.q, args.P, args.D, args.Q
            print(f"\nFitting SARIMA({p},{d},{q})x({P},{D},{Q})_{args.m}...")
            model = fit_sarima(series, p, d, q, P, D, Q, args.m)
        forecast = forecast_sarima(model, h)
    else:  # holtwinters
        m = args.m
        print(f"\nFitting Holt-Winters (m={m}, additive)...")
        model = fit_holt_winters(series, m)
        forecast = forecast_hw(model, h)

    # Print forecast
    print(f"\n{'─' * 50}")
    print(f"  {h}-step Forecast")
    print(f"{'─' * 50}")
    print(f"  {'Horizon':>8}  {'Point':>10}  {'Lower':>10}  {'Upper':>10}")
    for i in range(h):
        print(f"  {i+1:>8}  {forecast['point'][i]:>10.2f}  {forecast['lower'][i]:>10.2f}  {forecast['upper'][i]:>10.2f}")
    print(f"{'─' * 50}")
    print(f"  Confidence level: {(1 - forecast['alpha']) * 100:.0f}%")

    # ASCII plot
    if args.plot:
        print(f"\n{ascii_plot(series, title='Original series')}")
        forecast_with_history = series + forecast["point"]
        print(f"\n{ascii_plot(forecast_with_history, title=f'Forecast (h={h})')}")

    # Backtest
    if args.backtest:
        print(f"\nRolling backtest (h={h})...")

        def fit_fn(train):
            if args.model == "arima":
                return fit_arima(train, args.p, args.d, args.q)
            elif args.model == "sarima":
                return fit_sarima(train, args.p, args.d, args.q, args.P, args.D, args.Q, args.m)
            else:
                return fit_holt_winters(train, args.m)

        def forecast_fn(model, h):
            if args.model == "arima":
                return forecast_arima(model, h)["point"]
            elif args.model == "sarima":
                return forecast_sarima(model, h)["point"]
            else:
                return forecast_hw(model, h)["point"]

        bt = rolling_backtest(series, fit_fn, forecast_fn, h=h, min_train=args.min_train)
        if bt["origins"] > 0:
            s = bt["summary"]
            print(f"  Origins tested: {bt['origins']}")
            print(f"  MAE:  {s['mae']:.4f}")
            print(f"  RMSE: {s['rmse']:.4f}")
            print(f"  MAPE: {s['mape']:.2f}%")
        else:
            print("  No valid origins — series too short.")


if __name__ == "__main__":
    main()
