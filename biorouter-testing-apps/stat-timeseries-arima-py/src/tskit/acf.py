"""Autocorrelation, partial autocorrelation, and stationarity tests.

Public API
----------
- acf(x, nlags, d)        — sample autocorrelation function
- pacf(x, nlags)          — partial autocorrelation via Durbin-Levinson
- adf_test(x, maxlag)     — augmented Dickey-Fuller stationarity test
"""

from __future__ import annotations

from .numerics import (
    acf as _acf,
    pacf as _pacf,
    adf_test as _adf,
    diff,
    to_vec,
)

__all__ = ["acf", "pacf", "adf_test"]


def acf(x, nlags: int = 40, d: int = 0) -> list[float]:
    """Return sample ACF lags 0 … nlags.

    Parameters
    ----------
    x : array-like
        Time series.
    nlags : int
        Number of lags.
    d : int
        Difference order before computing ACF (0 = none).
    """
    return _acf(to_vec(x), nlags, d)


def pacf(x, nlags: int = 40) -> list[float]:
    """Return sample PACF lags 0 … nlags via Durbin-Levinson recursion."""
    return _pacf(to_vec(x), nlags)


def adf_test(x, maxlag: int | None = None) -> dict:
    """Augmented Dickey-Fuller stationarity test.

    Returns
    -------
    dict with keys:
      statistic  — ADF t-statistic
      lags       — number of lags used
      critical   — dict of approximate critical values
      p_value    — approximate p-value
      reject_5pct — True if H0 of unit root is rejected at 5 %
    """
    return _adf(to_vec(x), maxlag)
