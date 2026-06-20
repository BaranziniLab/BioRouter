# tskit — Classical Time-Series Forecasting Toolkit

A pure-Python (with optional NumPy acceleration) implementation of classical time-series models, fitting, forecasting, and evaluation tools.

## Features

### Models
- **AR(p)** — Autoregressive via Yule-Walker and least-squares estimation
- **MA(q)** — Moving average via conditional sum-of-squares and approximate MLE
- **ARMA(p,q)** — Combined AR+MA with iterative estimation
- **ARIMA(p,d,q)** — Differencing + ARMA
- **SARIMA(p,d,q)×(P,D,Q)_m** — Seasonal ARIMA with regular and seasonal components
- **Holt-Winters** — Exponential smoothing (additive & multiplicative, optional damped trend)

### Analysis Tools
- **ACF / PACF** — Sample autocorrelation and partial autocorrelation (Durbin-Levinson)
- **ADF test** — Augmented Dickey-Fuller stationarity test
- **Differencing / Integration** — Regular and seasonal, with exact round-trip
- **Automatic order selection** — Grid search over (p,d,q) with AIC/BIC criterion

### Evaluation
- **Prediction intervals** — Asymptotic forecast intervals for all models
- **Rolling-origin backtest** — Expanding-window evaluation
- **Error metrics** — MAE, RMSE, MAPE

### CLI
```bash
python -m tskit.cli data.csv --model arima --p 2 --d 1 --q 1 --h 10 --auto --plot
```

## Installation

```bash
pip install -e ".[dev]"
```

## Usage (Python)

```python
from tskit.arima import fit_arima, forecast_arima
from tskit.holtwinters import fit_holt_winters, forecast_hw

# ARIMA
model = fit_arima(series, p=2, d=1, q=1)
fc = forecast_arima(model, steps=10)
print(fc["point"], fc["lower"], fc["upper"])

# Holt-Winters
model = fit_holt_winters(series, m=12, method="additive")
fc = forecast_hw(model, steps=12)
```

## Running Tests

```bash
pytest -v
```

## Project Structure

```
src/tskit/
  numerics.py    — Core linear algebra, statistics, simulation
  acf.py         — ACF, PACF, ADF test
  ar.py          — AR model fitting and forecasting
  ma.py          — MA model fitting and forecasting
  arima.py       — ARIMA (differencing + ARMA)
  sarima.py      — Seasonal SARIMA
  holtwinters.py — Holt-Winters exponential smoothing
  autoorder.py   — Automatic order selection (AIC/BIC)
  backtest.py    — Rolling-origin backtesting, error metrics
  cli.py         — Command-line interface

tests/
  test_numerics.py  — Core numerics
  test_acf.py       — ACF/PACF/stationarity
  test_ar.py        — AR fitting
  test_ma.py        — MA fitting
  test_arima.py     — ARIMA integration
  test_sarima.py    — SARIMA
  test_holtwinters.py — Holt-Winters
  test_autoorder.py — Auto order selection
  test_backtest.py  — Backtesting and metrics
  test_cli.py       — CLI integration
```

## Design Philosophy

- **Pure Python first** — No NumPy required; optional acceleration via NumPy when available.
- **Classical methods** — Implements foundational models from scratch for transparency and education.
- **Incrementally testable** — Each module is self-contained with clear interfaces.
