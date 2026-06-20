"""
Jackknife resampling methods.

Implements leave-one-out (LOO) jackknife and delete-d jackknife for
bias and variance estimation.
"""

from typing import Callable, Optional, Tuple
import numpy as np
from numpy.typing import ArrayLike

from .utils import (
    validate_data,
    validate_statistic,
    compute_bias,
    ResamplingResult
)


def jackknife(
    data: ArrayLike,
    stat: Callable[[np.ndarray], float],
    method: str = 'loo'
) -> 'JackknifeResult':
    """
    Perform jackknife resampling.
    
    The jackknife estimates bias and variance by systematically leaving out
    observations.
    
    Args:
        data: 1D array of observations
        stat: Statistic function
        method: Jackknife method ('loo' for leave-one-out, 'delete-d')
        
    Returns:
        JackknifeResult with bias and variance estimates
        
    Example:
        >>> data = np.random.normal(0, 1, 100)
        >>> result = jackknife(data, np.mean)
        >>> print(f"Estimate: {result.estimate:.3f}")
        >>> print(f"Bias: {result.bias:.3f}")
        >>> print(f"SE: {result.std_error:.3f}")
    """
    data = validate_data(data)
    stat = validate_statistic(stat)
    
    if method == 'loo':
        return _jackknife_loo(data, stat)
    elif method == 'delete-d':
        return _jackknife_delete_d(data, stat)
    else:
        raise ValueError(f"Unknown method: {method}. Use 'loo' or 'delete-d'.")


def _jackknife_loo(data: np.ndarray, stat: Callable) -> 'JackknifeResult':
    """
    Leave-one-out jackknife.
    
    Creates n jackknife samples, each omitting one observation.
    
    Args:
        data: Original data
        stat: Statistic function
        
    Returns:
        JackknifeResult
    """
    n = len(data)
    observed = stat(data)
    
    # Compute jackknife replicates
    jack_stats = np.zeros(n)
    for i in range(n):
        loo = np.delete(data, i)
        jack_stats[i] = stat(loo)
    
    # Jackknife estimate of the statistic
    jack_mean = np.mean(jack_stats)
    
    # Bias estimate: (n-1) * (T_jack - T)
    bias = (n - 1) * (jack_mean - observed)
    
    # Variance estimate: ((n-1)/n) * sum((T_jack_i - T_jack)^2)
    variance = ((n - 1) / n) * np.sum((jack_stats - jack_mean) ** 2)
    std_error = np.sqrt(variance)
    
    # Bias-corrected estimate
    bias_corrected = observed - bias
    
    return JackknifeResult(
        estimate=observed,
        jackknife_stats=jack_stats,
        bias=bias,
        std_error=std_error,
        variance=variance,
        bias_corrected=bias_corrected,
        method='loo',
        n_resamples=n
    )


def _jackknife_delete_d(data: np.ndarray, stat: Callable) -> 'JackknifeResult':
    """
    Delete-d jackknife.
    
    Creates jackknife samples by deleting d observations at a time.
    Uses d = floor(n/4) as default.
    
    Args:
        data: Original data
        stat: Statistic function
        
    Returns:
        JackknifeResult
    """
    n = len(data)
    observed = stat(data)
    
    # Choose d (number of observations to delete)
    d = max(1, n // 4)
    
    # For large n, use a subsample of all possible delete-d samples
    # to keep computation tractable
    max_combos = min(1000, n)  # Limit number of combinations
    rng = np.random.default_rng(42)
    
    # Generate delete-d samples
    jack_stats = []
    
    if n <= 20:
        # For small n, enumerate all combinations
        from itertools import combinations
        combos = list(combinations(range(n), d))
        if len(combos) > max_combos:
            # Random subsample of combinations
            indices = rng.choice(len(combos), size=max_combos, replace=False)
            combos = [combos[i] for i in indices]
    else:
        # For large n, random subsample
        for _ in range(max_combos):
            indices = rng.choice(n, size=d, replace=False)
            combos = [tuple(indices)]
    
    for combo in combos:
        mask = np.ones(n, dtype=bool)
        mask[list(combo)] = False
        jack_sample = data[mask]
        jack_stats.append(stat(jack_sample))
    
    jack_stats = np.array(jack_stats)
    jack_mean = np.mean(jack_stats)
    
    # Variance estimate for delete-d jackknife
    # Using the formula from Shao and Tu (1995)
    m = len(jack_stats)
    variance = ((n - d) / (d * m)) * np.sum((jack_stats - jack_mean) ** 2)
    std_error = np.sqrt(max(0, variance))
    
    # Bias estimate
    bias = (n / d) * (jack_mean - observed) if d > 0 else 0.0
    bias_corrected = observed - bias
    
    return JackknifeResult(
        estimate=observed,
        jackknife_stats=jack_stats,
        bias=bias,
        std_error=std_error,
        variance=variance,
        bias_corrected=bias_corrected,
        method='delete-d',
        n_resamples=m
    )


def jackknife_variance(
    data: ArrayLike,
    stat: Callable[[np.ndarray], float]
) -> float:
    """
    Compute jackknife variance estimate.
    
    Args:
        data: 1D array of observations
        stat: Statistic function
        
    Returns:
        Jackknife variance estimate
    """
    result = jackknife(data, stat, method='loo')
    return result.variance


def jackknife_bias(
    data: ArrayLike,
    stat: Callable[[np.ndarray], float]
) -> float:
    """
    Compute jackknife bias estimate.
    
    Args:
        data: 1D array of observations
        stat: Statistic function
        
    Returns:
        Jackknife bias estimate
    """
    result = jackknife(data, stat, method='loo')
    return result.bias


def jackknife_ci(
    data: ArrayLike,
    stat: Callable[[np.ndarray], float],
    ci_level: float = 0.95
) -> Tuple[float, float]:
    """
    Compute jackknife confidence interval.
    
    Uses the normal approximation based on jackknife variance.
    
    Args:
        data: 1D array of observations
        stat: Statistic function
        ci_level: Confidence level (0-1)
        
    Returns:
        Tuple of (lower, upper) confidence bounds
    """
    from scipy import stats as scipy_stats
    
    result = jackknife(data, stat, method='loo')
    
    # Normal approximation
    z = scipy_stats.norm.ppf(1 - (1 - ci_level) / 2)
    margin = z * result.std_error
    
    lower = result.bias_corrected - margin
    upper = result.bias_corrected + margin
    
    return lower, upper


class JackknifeResult(ResamplingResult):
    """Result of a jackknife analysis."""
    
    def __init__(
        self,
        estimate: float,
        jackknife_stats: np.ndarray,
        bias: float,
        std_error: float,
        variance: float,
        bias_corrected: float,
        method: str = 'loo',
        n_resamples: Optional[int] = None
    ):
        """
        Initialize JackknifeResult.
        
        Args:
            estimate: Original statistic estimate
            jackknife_stats: Array of jackknife replicates
            bias: Estimated bias
            std_error: Estimated standard error
            variance: Estimated variance
            bias_corrected: Bias-corrected estimate
            method: Jackknife method used
            n_resamples: Number of jackknife samples
        """
        super().__init__(
            estimate=estimate,
            bootstrap_stats=jackknife_stats,
            std_error=std_error,
            bias=bias,
            n_resamples=n_resamples,
            method=method
        )
        self.variance = variance
        self.bias_corrected = bias_corrected
        self.jackknife_stats = jackknife_stats
    
    def summary(self) -> str:
        """Return a summary string of the jackknife results."""
        lines = ["Jackknife Result"]
        lines.append(f"  Estimate: {self.estimate:.6f}")
        lines.append(f"  Bias: {self.bias:.6f}")
        lines.append(f"  Bias-corrected: {self.bias_corrected:.6f}")
        lines.append(f"  Std Error: {self.std_error:.6f}")
        lines.append(f"  Variance: {self.variance:.6f}")
        lines.append(f"  Method: {self.method}")
        lines.append(f"  Samples: {self.n_resamples}")
        return "\n".join(lines)
    
    def __repr__(self) -> str:
        return self.summary()
