"""
MCMC diagnostics.

Provides functions for evaluating MCMC chain quality:
- Trace summaries (mean, std, quantiles)
- Effective sample size (ESS)
- Gelman-Rubin R-hat (between-chain / within-chain variance)
- Autocorrelation function and plots
- Acceptance rate
- Burn-in and thinning utilities
- Geweke diagnostic
"""

from __future__ import annotations

import math
from typing import Dict, List, Optional, Tuple, Union

import numpy as np


# ---------------------------------------------------------------------------
# Effective Sample Size (ESS)
# ---------------------------------------------------------------------------

def compute_ess(chain: np.ndarray) -> float:
    """
    Compute effective sample size using initial monotone sequence estimator.

    Parameters
    ----------
    chain : np.ndarray of shape (n,)
        A single MCMC chain (1D).

    Returns
    -------
    float
        Estimated effective sample size.
    """
    chain = np.asarray(chain, dtype=float)
    n = len(chain)
    if n < 10:
        return float(n)

    # subtract mean
    chain = chain - chain.mean()

    # compute autocorrelation via FFT
    acf = _autocorrelation(chain)
    n_lag = len(acf)

    # initial monotone sequence estimator (Geyer 1992)
    # sum pairs of consecutive autocorrelations until they become negative
    ess = n
    tau = 0.0
    g = 0.0
    prev_gamma = acf[0]

    for lag in range(1, n_lag, 2):
        if lag + 1 < n_lag:
            pair = acf[lag] + acf[lag + 1]
        else:
            pair = acf[lag]

        if pair < 0:
            break

        g += pair
        tau += lag * pair

    if g > 0:
        ess = n / (1 + 2 * g)
    else:
        ess = float(n)

    return max(ess, 1.0)


def _autocorrelation(chain: np.ndarray, max_lag: Optional[int] = None) -> np.ndarray:
    """Compute autocorrelation function using FFT (normalised, lag 0 = 1)."""
    n = len(chain)
    if max_lag is None:
        max_lag = min(n // 2, 500)

    # pad to next power of 2 for FFT efficiency
    nfft = int(2 ** math.ceil(math.log2(2 * n)))
    fft_chain = np.fft.fft(chain, n=nfft)
    acf_full = np.real(np.fft.ifft(fft_chain * np.conj(fft_chain)))[:n]
    acf_full = acf_full / acf_full[0]

    return acf_full[:max_lag + 1]


def autocorrelation(chain: np.ndarray, max_lag: Optional[int] = None) -> np.ndarray:
    """Public interface: compute normalised autocorrelation function."""
    return _autocorrelation(np.asarray(chain, dtype=float), max_lag)


# ---------------------------------------------------------------------------
# Gelman-Rubin R-hat
# ---------------------------------------------------------------------------

def compute_rhat(
    chains: Union[np.ndarray, Dict[str, np.ndarray]],
    param_name: Optional[str] = None,
) -> float:
    """
    Compute the Gelman-Rubin R-hat diagnostic (Brooks & Gelman 1998).

    Parameters
    ----------
    chains : np.ndarray of shape (n_chains, n_samples) or dict
        If dict, param_name must be provided.
    param_name : str, optional
        Key into the chains dict.

    Returns
    -------
    float
        R-hat value. Values close to 1.0 indicate convergence.
    """
    if isinstance(chains, dict):
        if param_name is None:
            raise ValueError("param_name required when chains is a dict")
        chain_array = np.asarray(chains[param_name], dtype=float)
    else:
        chain_array = np.asarray(chains, dtype=float)

    if chain_array.ndim == 1:
        chain_array = chain_array.reshape(1, -1)

    m, n = chain_array.shape  # m chains, n samples each

    if m < 2 or n < 4:
        return 1.0  # cannot compute with too few chains/samples

    # between-chain variance B
    chain_means = chain_array.mean(axis=1)
    overall_mean = chain_means.mean()
    B = n * chain_means.var(ddof=1)

    # within-chain variance W
    chain_vars = chain_array.var(axis=1, ddof=1)
    W = chain_vars.mean()

    # pooled variance estimate
    var_hat = (1 - 1.0 / n) * W + (1.0 / n) * B

    if W <= 0:
        return 1.0

    rhat = math.sqrt(var_hat / W)
    return rhat


# ---------------------------------------------------------------------------
# Acceptance rate
# ---------------------------------------------------------------------------

def compute_acceptance_rate(chain_metadata: dict) -> float:
    """
    Extract acceptance rate from sampler output metadata.

    Parameters
    ----------
    chain_metadata : dict
        Dictionary potentially containing '_acceptance_rate' key.

    Returns
    -------
    float
        Mean acceptance rate across chains.
    """
    if "_acceptance_rate" not in chain_metadata:
        return float("nan")
    rates = chain_metadata["_acceptance_rate"]
    return float(np.mean(rates))


# ---------------------------------------------------------------------------
# Trace summaries
# ---------------------------------------------------------------------------

def trace_summary(
    chain: np.ndarray,
    quantiles: Optional[List[float]] = None,
) -> dict:
    """
    Compute summary statistics for an MCMC chain.

    Parameters
    ----------
    chain : np.ndarray
        1D array of samples.
    quantiles : list of float, optional
        Quantiles to compute (default: [0.025, 0.25, 0.5, 0.75, 0.975]).

    Returns
    -------
    dict with keys: mean, std, min, max, q{pct} for each quantile.
    """
    chain = np.asarray(chain, dtype=float)
    if quantiles is None:
        quantiles = [0.025, 0.25, 0.5, 0.75, 0.975]

    summary = {
        "mean": float(chain.mean()),
        "std": float(chain.std(ddof=1)),
        "min": float(chain.min()),
        "max": float(chain.max()),
        "n": len(chain),
    }

    qs = np.quantile(chain, quantiles)
    for q, val in zip(quantiles, qs):
        summary[f"q{q:.3f}"] = float(val)

    return summary


# ---------------------------------------------------------------------------
# Geweke diagnostic
# ---------------------------------------------------------------------------

def geweke_diagnostic(
    chain: np.ndarray,
    first_frac: float = 0.1,
    last_frac: float = 0.5,
) -> float:
    """
    Geweke (1992) diagnostic comparing means of early and late chain segments.

    Returns z-score; |z| > 2 suggests non-convergence.
    """
    chain = np.asarray(chain, dtype=float)
    n = len(chain)
    n1 = int(n * first_frac)
    n2_start = int(n * (1 - last_frac))

    if n1 < 10 or (n - n2_start) < 10:
        return 0.0

    x1 = chain[:n1]
    x2 = chain[n2_start:]

    # Spectral density at frequency 0 (using initial monotone sequence)
    sd1 = _spectral_density_at_zero(x1)
    sd2 = _spectral_density_at_zero(x2)

    m1, m2 = x1.mean(), x2.mean()
    se = math.sqrt(sd1 / len(x1) + sd2 / len(x2))

    if se < 1e-300:
        return 0.0

    return (m1 - m2) / se


def _spectral_density_at_zero(chain: np.ndarray) -> float:
    """Estimate spectral density at frequency 0 using initial positive sequence."""
    acf = _autocorrelation(chain, max_lag=min(len(chain) // 4, 200))
    n_lag = len(acf)

    # sum pairs of consecutive autocorrelations
    sd0 = acf[0]
    for lag in range(1, n_lag, 2):
        if lag + 1 < n_lag:
            pair = acf[lag] + acf[lag + 1]
        else:
            pair = acf[lag]
        if pair < 0:
            break
        sd0 += 2 * pair

    return max(sd0, 1e-300)


# ---------------------------------------------------------------------------
# Burn-in / thinning
# ---------------------------------------------------------------------------

def burn_in(chains: Dict[str, np.ndarray], n_burn: int) -> Dict[str, np.ndarray]:
    """Remove burn-in samples from chains."""
    return {name: chain[:, n_burn:] if chain.ndim > 1 else chain[n_burn:]
            for name, chain in chains.items() if not name.startswith("_")}


def thin(chains: Dict[str, np.ndarray], factor: int) -> Dict[str, np.ndarray]:
    """Thin chains by keeping every factor-th sample."""
    return {name: chain[:, ::factor] if chain.ndim > 1 else chain[::factor]
            for name, chain in chains.items() if not name.startswith("_")}


# ---------------------------------------------------------------------------
# Summary across chains
# ---------------------------------------------------------------------------

def multi_chain_summary(
    chains: Dict[str, np.ndarray],
) -> Dict[str, dict]:
    """
    Compute summary statistics for each parameter across all chains.

    Parameters
    ----------
    chains : dict
        {param_name: np.ndarray of shape (n_chains, n_samples)}

    Returns
    -------
    dict : {param_name: summary_dict}
    """
    summaries = {}
    for name, chain in chains.items():
        if name.startswith("_"):
            continue
        chain = np.asarray(chain, dtype=float)
        if chain.ndim == 1:
            chain = chain.reshape(1, -1)
        # pool all chains
        pooled = chain.flatten()
        summary = trace_summary(pooled)
        summary["rhat"] = compute_rhat(chains, name)
        summary["ess"] = compute_ess(pooled)
        summaries[name] = summary
    return summaries
