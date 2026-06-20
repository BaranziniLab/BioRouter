"""
Bootstrap confidence intervals.

Implements percentile, basic, BCa (bias-corrected and accelerated),
and bootstrap-t confidence intervals.
"""

from typing import Any, Callable, Optional, Tuple, Union
from scipy import stats
import numpy as np
from numpy.typing import ArrayLike

from .utils import (
    validate_data,
    validate_statistic,
    create_rng,
    compute_std_error,
    ResamplingResult
)


def percentile_ci(
    bootstrap_stats: ArrayLike,
    ci_level: float = 0.95
) -> Tuple[float, float]:
    """
    Compute percentile confidence interval.
    
    The percentile method uses quantiles of the bootstrap distribution.
    
    Args:
        bootstrap_stats: Array of bootstrap statistics
        ci_level: Confidence level (0-1)
        
    Returns:
        Tuple of (lower, upper) confidence bounds
        
    Example:
        >>> boot_stats = np.random.normal(0, 1, 9999)
        >>> lower, upper = percentile_ci(boot_stats, 0.95)
    """
    bootstrap_stats = np.asarray(bootstrap_stats, dtype=float)
    alpha = 1 - ci_level
    lower = np.percentile(bootstrap_stats, 100 * alpha / 2)
    upper = np.percentile(bootstrap_stats, 100 * (1 - alpha / 2))
    return lower, upper


def basic_ci(
    observed: float,
    bootstrap_stats: ArrayLike,
    ci_level: float = 0.95
) -> Tuple[float, float]:
    """
    Compute basic (pivotal) confidence interval.
    
    The basic method uses the pivot: 2*T - T*
    
    Args:
        observed: Observed statistic
        bootstrap_stats: Array of bootstrap statistics
        ci_level: Confidence level (0-1)
        
    Returns:
        Tuple of (lower, upper) confidence bounds
    """
    bootstrap_stats = np.asarray(bootstrap_stats, dtype=float)
    alpha = 1 - ci_level
    
    # Use quantiles of bootstrap distribution and pivot
    # The basic CI is: [2T - T*_{1-alpha/2}, 2T - T*_{alpha/2}]
    lower = 2 * observed - np.percentile(bootstrap_stats, 100 * (1 - alpha / 2))
    upper = 2 * observed - np.percentile(bootstrap_stats, 100 * alpha / 2)
    
    return lower, upper


def bca_ci(
    data: ArrayLike,
    stat: Callable[[np.ndarray], float],
    bootstrap_stats: ArrayLike,
    ci_level: float = 0.95,
    observed: Optional[float] = None
) -> Tuple[float, float]:
    """
    Compute BCa (bias-corrected and accelerated) confidence interval.
    
    The BCa interval applies two corrections:
    - Bias correction (z0): adjusts for median bias
    - Acceleration (a): adjusts for skewness
    
    Args:
        data: Original data (needed for jackknife acceleration)
        stat: Statistic function
        bootstrap_stats: Array of bootstrap statistics
        ci_level: Confidence level (0-1)
        observed: Observed statistic (computed if not provided)
        
    Returns:
        Tuple of (lower, upper) confidence bounds
        
    References:
        Efron, B. (1993). Second-order accuracy and the BCa method.
    """
    data = validate_data(data)
    bootstrap_stats = np.asarray(bootstrap_stats, dtype=float)
    n = len(data)
    
    if observed is None:
        observed = stat(data)
    
    # Bias correction factor z0
    # z0 = Φ^{-1}(proportion of bootstrap stats ≤ observed)
    prop_le = np.mean(bootstrap_stats <= observed)
    # Handle edge cases
    prop_le = np.clip(prop_le, 1e-10, 1 - 1e-10)
    z0 = stats.norm.ppf(prop_le)
    
    # Acceleration factor via jackknife
    jack_stats = np.zeros(n)
    for i in range(n):
        # Leave-one-out sample
        loo = np.delete(data, i)
        jack_stats[i] = stat(loo)
    
    jack_mean = np.mean(jack_stats)
    
    # Numerator: sum of (jack_mean - jack_stats)^3
    num = np.sum((jack_mean - jack_stats) ** 3)
    
    # Denominator: 6 * (sum of (jack_mean - jack_stats)^2)^(3/2)
    denom = 6 * (np.sum((jack_mean - jack_stats) ** 2)) ** 1.5
    
    if abs(denom) < 1e-20:
        # No acceleration
        a_hat = 0.0
    else:
        a_hat = num / denom
    
    # BCa percentiles
    alpha = 1 - ci_level
    z_alpha = stats.norm.ppf(alpha / 2)
    z_1_alpha = stats.norm.ppf(1 - alpha / 2)
    
    # Adjusted percentiles
    p_lower = stats.norm.cdf(
        z0 + (z0 + z_alpha) / (1 - a_hat * (z0 + z_alpha))
    )
    p_upper = stats.norm.cdf(
        z0 + (z0 + z_1_alpha) / (1 - a_hat * (z0 + z_1_alpha))
    )
    
    # Convert to percentiles
    lower = np.percentile(bootstrap_stats, 100 * p_lower)
    upper = np.percentile(bootstrap_stats, 100 * p_upper)
    
    return lower, upper


def bootstrap_t_ci(
    data: ArrayLike,
    stat: Callable[[np.ndarray], float],
    bootstrap_stats: ArrayLike,
    ci_level: float = 0.95,
    observed: Optional[float] = None,
    n_mc: int = 200
) -> Tuple[float, float]:
    """
    Compute bootstrap-t (studentized) confidence interval.
    
    This is a second-order accurate interval that studentizes the statistic.
    
    Args:
        data: Original data
        stat: Statistic function
        bootstrap_stats: Array of bootstrap statistics
        ci_level: Confidence level (0-1)
        observed: Observed statistic
        n_mc: Number of samples for variance estimation
        
    Returns:
        Tuple of (lower, upper) confidence bounds
        
    References:
        Efron, B. (1981). Nonparametric standard errors and confidence intervals.
    """
    data = validate_data(data)
    stat = validate_statistic(stat)
    bootstrap_stats = np.asarray(bootstrap_stats, dtype=float)
    rng = create_rng(42)
    n = len(data)
    
    if observed is None:
        observed = stat(data)
    
    # Compute bootstrap t-statistics
    t_stats = np.zeros(len(bootstrap_stats))
    
    for b, boot_sample in enumerate(_generate_bootstrap_samples(data, bootstrap_stats, rng)):
        boot_obs = stat(boot_sample)
        
        # Estimate variance of boot_sample using jackknife
        jack_vars = np.zeros(n)
        for i in range(n):
            loo = np.delete(boot_sample, i)
            jack_vars[i] = stat(loo)
        
        # Jackknife variance estimate
        jack_var = np.var(jack_vars, ddof=1) * (n - 1)
        if jack_var <= 0:
            jack_var = np.var(bootstrap_stats, ddof=1)
        
        # Studentize
        se = np.sqrt(jack_var)
        if se > 0:
            t_stats[b] = (boot_obs - observed) / se
        else:
            t_stats[b] = 0.0
    
    # Quantiles of t distribution
    alpha = 1 - ci_level
    t_lower = np.percentile(t_stats, 100 * alpha / 2)
    t_upper = np.percentile(t_stats, 100 * (1 - alpha / 2))
    
    # Estimate SE for observed
    jack_stats_obs = np.zeros(n)
    for i in range(n):
        loo = np.delete(data, i)
        jack_stats_obs[i] = stat(loo)
    
    se_obs = np.sqrt(np.var(jack_stats_obs, ddof=1) * (n - 1))
    
    # CI bounds
    lower = observed - t_upper * se_obs
    upper = observed - t_lower * se_obs
    
    return lower, upper


def _generate_bootstrap_samples(
    data: np.ndarray,
    bootstrap_stats: np.ndarray,
    rng: np.random.Generator
):
    """Generate bootstrap samples matching existing bootstrap statistics."""
    n = len(data)
    B = len(bootstrap_stats)
    
    for b in range(B):
        indices = rng.integers(0, n, size=n)
        yield data[indices]


def bootstrap_ci(
    data: ArrayLike,
    stat: Callable[[np.ndarray], float],
    method: str = 'percentile',
    ci_level: float = 0.95,
    B: int = 9999,
    seed: Optional[int] = None,
    rng: Optional[np.random.Generator] = None,
    **kwargs
) -> 'CIResult':
    """
    Compute bootstrap confidence interval.
    
    This is the main API for computing CIs. It performs bootstrap resampling
    and computes the confidence interval using the specified method.
    
    Args:
        data: 1D array of observations
        stat: Statistic function
        method: CI method ('percentile', 'basic', 'bca', 'bootstrap_t')
        ci_level: Confidence level (0-1)
        B: Number of bootstrap resamples
        seed: Random seed for reproducibility
        rng: Pre-initialized random generator
        **kwargs: Additional arguments
        
    Returns:
        CIResult object with interval and statistics
        
    Example:
        >>> data = np.random.normal(0, 1, 100)
        >>> result = bootstrap_ci(data, np.mean, method='bca', B=9999)
        >>> print(f"95% CI: [{result.ci_lower:.3f}, {result.ci_upper:.3f}]")
    """
    data = validate_data(data)
    stat = validate_statistic(stat)
    rng = create_rng(seed, rng)
    n = len(data)
    
    # Compute observed statistic
    observed = stat(data)
    
    # Generate bootstrap statistics
    bootstrap_stats = np.zeros(B)
    for b in range(B):
        indices = rng.integers(0, n, size=n)
        boot_sample = data[indices]
        bootstrap_stats[b] = stat(boot_sample)
    
    # Compute CI based on method
    if method == 'percentile':
        ci_lower, ci_upper = percentile_ci(bootstrap_stats, ci_level)
    elif method == 'basic':
        ci_lower, ci_upper = basic_ci(observed, bootstrap_stats, ci_level)
    elif method == 'bca':
        ci_lower, ci_upper = bca_ci(data, stat, bootstrap_stats, ci_level, observed)
    elif method == 'bootstrap_t':
        ci_lower, ci_upper = bootstrap_t_ci(
            data, stat, bootstrap_stats, ci_level, observed
        )
    else:
        raise ValueError(
            f"Unknown method: {method}. Use 'percentile', 'basic', 'bca', or 'bootstrap_t'."
        )
    
    return CIResult(
        estimate=observed,
        bootstrap_stats=bootstrap_stats,
        ci_lower=ci_lower,
        ci_upper=ci_upper,
        ci_level=ci_level,
        method=method,
        n_resamples=B,
        seed=seed,
        std_error=compute_std_error(bootstrap_stats),
        bias=float(np.mean(bootstrap_stats) - observed)
    )


class CIResult(ResamplingResult):
    """Result of a bootstrap confidence interval analysis."""
    
    def __init__(
        self,
        estimate: float,
        bootstrap_stats: np.ndarray,
        ci_lower: float,
        ci_upper: float,
        ci_level: float = 0.95,
        method: str = 'percentile',
        n_resamples: int = 9999,
        seed: Optional[int] = None,
        std_error: Optional[float] = None,
        bias: Optional[float] = None
    ):
        """
        Initialize CIResult.
        
        Args:
            estimate: Observed statistic
            bootstrap_stats: Array of bootstrap statistics
            ci_lower: Lower CI bound
            ci_upper: Upper CI bound
            ci_level: Confidence level
            method: CI method used
            n_resamples: Number of bootstrap resamples
            seed: Random seed used
            std_error: Bootstrap standard error
            bias: Bootstrap bias estimate
        """
        super().__init__(
            estimate=estimate,
            bootstrap_stats=bootstrap_stats,
            std_error=std_error,
            bias=bias,
            ci_lower=ci_lower,
            ci_upper=ci_upper,
            ci_level=ci_level,
            method=method,
            n_resamples=n_resamples,
            seed=seed
        )
    
    def coverage_check(self, true_value: float) -> bool:
        """Check if the CI contains the true value."""
        return self.ci_lower <= true_value <= self.ci_upper
    
    def ci_width(self) -> float:
        """Return the width of the confidence interval."""
        return self.ci_upper - self.ci_lower
