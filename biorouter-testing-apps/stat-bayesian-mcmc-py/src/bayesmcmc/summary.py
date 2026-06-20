"""
Posterior summary statistics.

Provides:
- Mean, median, mode (MAP)
- Credible intervals (equal-tailed)
- Highest Posterior Density (HPD) intervals
- Quantiles
- Full posterior report
"""

from __future__ import annotations

import math
from typing import Dict, List, Optional, Tuple, Union

import numpy as np


def posterior_mean(samples: np.ndarray) -> float:
    """Compute posterior mean."""
    return float(np.mean(samples))


def posterior_median(samples: np.ndarray) -> float:
    """Compute posterior median."""
    return float(np.median(samples))


def posterior_mode(samples: np.ndarray, n_bins: int = 100) -> float:
    """
    Estimate posterior mode via kernel density estimation.
    """
    samples = np.asarray(samples, dtype=float)
    # simple KDE-based mode estimate
    from numpy import histogram
    counts, bin_edges = histogram(samples, bins=n_bins)
    bin_centers = 0.5 * (bin_edges[:-1] + bin_edges[1:])
    idx = np.argmax(counts)
    return float(bin_centers[idx])


def credible_interval(
    samples: np.ndarray,
    level: float = 0.95,
) -> Tuple[float, float]:
    """
    Compute equal-tailed credible interval.

    Parameters
    ----------
    samples : np.ndarray
        Posterior samples.
    level : float
        Credible level (default 0.95 for 95% CI).

    Returns
    -------
    (lower, upper) bounds.
    """
    alpha = 1.0 - level
    lower = float(np.quantile(samples, alpha / 2))
    upper = float(np.quantile(samples, 1 - alpha / 2))
    return lower, upper


def hpd_interval(
    samples: np.ndarray,
    level: float = 0.95,
) -> Tuple[float, float]:
    """
    Compute the Highest Posterior Density (HPD) interval.

    The HPD is the narrowest interval containing the specified
    probability mass.
    """
    samples = np.sort(np.asarray(samples, dtype=float))
    n = len(samples)
    k = int(math.ceil(level * n))

    if k >= n:
        return float(samples[0]), float(samples[-1])

    # find the narrowest window of k consecutive samples
    widths = samples[k - 1:] - samples[:n - k + 1]
    idx = np.argmin(widths)

    return float(samples[idx]), float(samples[idx + k - 1])


def quantiles(
    samples: np.ndarray,
    probs: List[float] = None,
) -> Dict[str, float]:
    """
    Compute quantiles of posterior samples.

    Parameters
    ----------
    samples : np.ndarray
        Posterior samples.
    probs : list of float
        Quantile probabilities (default: [0.025, 0.25, 0.5, 0.75, 0.975]).

    Returns
    -------
    dict : {f"q{p:.3f}": value}
    """
    if probs is None:
        probs = [0.025, 0.25, 0.5, 0.75, 0.975]

    qs = np.quantile(samples, probs)
    return {f"q{p:.3f}": float(v) for p, v in zip(probs, qs)}


def posterior_summary(
    samples: np.ndarray,
    ci_level: float = 0.95,
    hpd_level: float = 0.95,
    quantile_probs: Optional[List[float]] = None,
) -> dict:
    """
    Comprehensive posterior summary.

    Returns
    -------
    dict with keys:
        mean, median, mode, std, ci_lower, ci_upper, hpd_lower, hpd_upper,
        q{pct} for each quantile, rhat, ess, n_samples
    """
    samples = np.asarray(samples, dtype=float)
    ci_lower, ci_upper = credible_interval(samples, ci_level)
    hpd_lower, hpd_upper = hpd_interval(samples, hpd_level)
    q = quantiles(samples, quantile_probs)

    summary = {
        "mean": posterior_mean(samples),
        "median": posterior_median(samples),
        "mode": posterior_mode(samples),
        "std": float(samples.std(ddof=1)),
        "ci_lower": ci_lower,
        "ci_upper": ci_upper,
        "ci_level": ci_level,
        "hpd_lower": hpd_lower,
        "hpd_upper": hpd_upper,
        "hpd_level": hpd_level,
        "n_samples": len(samples),
    }
    summary.update(q)
    return summary


def multi_param_summary(
    chains: Dict[str, np.ndarray],
    ci_level: float = 0.95,
    hpd_level: float = 0.95,
) -> Dict[str, dict]:
    """
    Summary for each parameter across all chains.

    Parameters
    ----------
    chains : dict
        {param_name: np.ndarray of shape (n_chains, n_samples) or (n_samples,)}

    Returns
    -------
    dict : {param_name: summary_dict}
    """
    summaries = {}
    for name, chain in chains.items():
        if name.startswith("_"):
            continue
        chain = np.asarray(chain, dtype=float)
        if chain.ndim > 1:
            chain = chain.flatten()
        summaries[name] = posterior_summary(chain, ci_level, hpd_level)
    return summaries


# ---------------------------------------------------------------------------
# Formatting for CLI / display
# ---------------------------------------------------------------------------

def format_summary_table(
    summaries: Dict[str, dict],
    width: int = 80,
) -> str:
    """
    Format posterior summaries as an ASCII table.

    Parameters
    ----------
    summaries : dict
        {param_name: summary_dict} from posterior_summary or multi_param_summary.
    width : int
        Table width.

    Returns
    -------
    str : Formatted table.
    """
    if not summaries:
        return "No summaries to display."

    # header
    header = f"{'Parameter':<20} {'Mean':>10} {'Std':>10} {'95% CI':>22} {'95% HPD':>22}"
    sep = "-" * len(header)

    lines = [sep, header, sep]

    for name, s in summaries.items():
        ci = f"[{s['ci_lower']:.4f}, {s['ci_upper']:.4f}]"
        hpd = f"[{s['hpd_lower']:.4f}, {s['hpd_upper']:.4f}]"
        row = f"{name:<20} {s['mean']:>10.4f} {s['std']:>10.4f} {ci:>22} {hpd:>22}"
        lines.append(row)

    lines.append(sep)
    return "\n".join(lines)


def format_trace_ascii(
    samples: np.ndarray,
    width: int = 60,
    height: int = 20,
    title: str = "Trace",
) -> str:
    """
    Render an ASCII trace plot.

    Parameters
    ----------
    samples : np.ndarray
        1D chain of samples.
    width : int
        Character width of the plot.
    height : int
        Character height of the plot.
    title : str
        Plot title.

    Returns
    -------
    str : ASCII art trace plot.
    """
    samples = np.asarray(samples, dtype=float)
    n = len(samples)

    # resample to fit width
    if n > width:
        indices = np.linspace(0, n - 1, width, dtype=int)
        plot_data = samples[indices]
    else:
        plot_data = samples
        width = len(plot_data)

    lo, hi = plot_data.min(), plot_data.max()
    if hi - lo < 1e-12:
        hi = lo + 1

    lines = [f"  {title}", f"  {hi:.3f} |"]

    grid = [[" " for _ in range(width)] for _ in range(height)]

    for x_idx, x in enumerate(plot_data):
        y_idx = int((x - lo) / (hi - lo) * (height - 1))
        y_idx = max(0, min(height - 1, y_idx))
        grid[height - 1 - y_idx][x_idx] = "●"

    for row in grid:
        lines.append("        |" + "".join(row))

    lines.append(f"  {lo:.3f} |" + "─" * width)
    lines.append("         " + "0" + " " * (width - 6) + f"sample={n}")
    return "\n".join(lines)


def format_histogram_ascii(
    samples: np.ndarray,
    width: int = 50,
    height: int = 15,
    bins: int = 20,
    title: str = "Posterior",
) -> str:
    """
    Render an ASCII histogram.

    Parameters
    ----------
    samples : np.ndarray
        1D array of posterior samples.
    width : int
        Max bar width in characters.
    height : int
        Number of rows.
    bins : int
        Number of bins.
    title : str
        Plot title.

    Returns
    -------
    str : ASCII art histogram.
    """
    samples = np.asarray(samples, dtype=float)
    counts, bin_edges = np.histogram(samples, bins=bins)
    max_count = counts.max()
    if max_count == 0:
        return f"  {title}\n  (empty)"

    lines = [f"  {title} (n={len(samples)}, bins={bins})", ""]

    for i, count in enumerate(counts):
        bar_len = int(count / max_count * width)
        bar = "█" * bar_len
        label = f"{bin_edges[i]:>8.3f}"
        lines.append(f"  {label} |{bar}")

    lines.append(f"  {bin_edges[-1]:>8.3f} |")
    lines.append(f"           {'─' * width}")
    lines.append(f"           {'count':^{width}}")

    return "\n".join(lines)
