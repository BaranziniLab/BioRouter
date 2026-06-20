"""
Utility functions for resampling methods.

Provides helper functions for random number generation, data validation,
and common statistical computations.
"""

from typing import Any, Callable, Optional, Sequence, Tuple, Union
import numpy as np
from numpy.typing import ArrayLike


def validate_data(data: ArrayLike) -> np.ndarray:
    """Validate and convert input data to numpy array.
    
    Args:
        data: Input data (list, array, or array-like)
        
    Returns:
        numpy array
        
    Raises:
        ValueError: If data is empty or contains non-numeric values
    """
    arr = np.asarray(data, dtype=float)
    if arr.size == 0:
        raise ValueError("Data cannot be empty")
    if not np.all(np.isfinite(arr)):
        raise ValueError("Data must contain only finite numeric values")
    return arr


def validate_statistic(stat: Callable) -> Callable:
    """Validate that a callable is a proper statistic function.
    
    Args:
        stat: Function that takes a 1D array and returns a scalar
        
    Returns:
        The validated function
        
    Raises:
        ValueError: If stat is not callable
    """
    if not callable(stat):
        raise ValueError("Statistic must be callable")
    return stat


def create_rng(
    seed: Optional[int] = None, 
    rng: Optional[np.random.Generator] = None
) -> np.random.Generator:
    """Create or validate a random number generator.
    
    Args:
        seed: Random seed for reproducibility
        rng: Existing numpy random generator
        
    Returns:
        numpy random Generator
        
    Raises:
        ValueError: If both seed and rng are provided
    """
    if seed is not None and rng is not None:
        raise ValueError("Cannot specify both seed and rng")
    
    if rng is not None:
        return rng
    
    return np.random.default_rng(seed)


def compute_bias(estimate: float, true_value: float) -> float:
    """Compute bias of an estimator.
    
    Args:
        estimate: Estimated value
        true_value: True parameter value
        
    Returns:
        Bias (estimate - true_value)
    """
    return estimate - true_value


def compute_mse(estimate: float, true_value: float) -> float:
    """Compute mean squared error.
    
    Args:
        estimate: Estimated value
        true_value: True parameter value
        
    Returns:
        MSE
    """
    return (estimate - true_value) ** 2


def compute_variance(values: ArrayLike) -> float:
    """Compute sample variance with Bessel's correction.
    
    Args:
        values: Array of values
        
    Returns:
        Sample variance (ddof=1)
    """
    arr = np.asarray(values, dtype=float)
    if arr.size < 2:
        raise ValueError("Need at least 2 values to compute variance")
    return float(np.var(arr, ddof=1))


def compute_std_error(bootstrap_stats: ArrayLike) -> float:
    """Compute standard error from bootstrap distribution.
    
    Args:
        bootstrap_stats: Array of bootstrap statistic values
        
    Returns:
        Standard error (sample std of bootstrap stats)
    """
    arr = np.asarray(bootstrap_stats, dtype=float)
    return float(np.std(arr, ddof=1))


def compute_percentile(values: ArrayLike, q: float) -> float:
    """Compute percentile using linear interpolation.
    
    Args:
        values: Array of values
        q: Percentile (0-100)
        
    Returns:
        Percentile value
    """
    arr = np.asarray(values, dtype=float)
    return float(np.percentile(arr, q))


def jackknife_resample(data: np.ndarray, indices: np.ndarray) -> np.ndarray:
    """Create a jackknife sample by leaving out specified indices.
    
    Args:
        data: Original data
        indices: Indices to exclude
        
    Returns:
        Data with specified indices removed
    """
    mask = np.ones(len(data), dtype=bool)
    mask[indices] = False
    return data[mask]


def block_resample(
    data: np.ndarray, 
    block_size: int, 
    n_blocks: Optional[int] = None,
    rng: Optional[np.random.Generator] = None
) -> np.ndarray:
    """Create a block bootstrap sample.
    
    Args:
        data: Original time series data
        block_size: Size of each block
        n_blocks: Number of blocks (default: ceil(n/block_size))
        rng: Random number generator
        
    Returns:
        Block bootstrap sample of approximately original length
    """
    n = len(data)
    if n_blocks is None:
        n_blocks = int(np.ceil(n / block_size))
    
    if rng is None:
        rng = np.random.default_rng()
    
    # Starting indices for each block
    max_start = n - block_size
    starts = rng.integers(0, max_start + 1, size=n_blocks)
    
    # Sample blocks
    blocks = []
    for start in starts:
        blocks.append(data[start:start + block_size])
    
    # Concatenate and truncate to original length
    result = np.concatenate(blocks)[:n]
    return result


def smooth_bootstrap_resample(
    data: np.ndarray,
    bandwidth: Optional[float] = None,
    rng: Optional[np.random.Generator] = None
) -> np.ndarray:
    """Create a smoothed bootstrap sample.
    
    Args:
        data: Original data
        bandwidth: Kernel bandwidth (default: std * n^(-1/5))
        rng: Random number generator
        
    Returns:
        Smoothed bootstrap sample
    """
    n = len(data)
    if rng is None:
        rng = np.random.default_rng()
    
    if bandwidth is None:
        # Silverman's rule of thumb
        bandwidth = np.std(data, ddof=1) * n ** (-1/5)
    
    # Sample indices with replacement
    indices = rng.integers(0, n, size=n)
    sampled = data[indices]
    
    # Add kernel noise (Gaussian kernel)
    noise = rng.normal(0, bandwidth, size=n)
    return sampled + noise


def parametric_bootstrap_resample(
    data: np.ndarray,
    model: str = 'normal',
    rng: Optional[np.random.Generator] = None,
    **params
) -> np.ndarray:
    """Create a parametric bootstrap sample from fitted distribution.
    
    Args:
        data: Original data (used to fit distribution if params not provided)
        model: Distribution type ('normal', 'exponential', 'poisson')
        rng: Random number generator
        **params: Distribution parameters (if not provided, fitted from data)
        
    Returns:
        Parametric bootstrap sample
    """
    if rng is None:
        rng = np.random.default_rng()
    
    n = len(data)
    
    if model == 'normal':
        mu = params.get('mu', np.mean(data))
        sigma = params.get('sigma', np.std(data, ddof=1))
        return rng.normal(mu, sigma, size=n)
    
    elif model == 'exponential':
        scale = params.get('scale', np.mean(data))
        if scale <= 0:
            raise ValueError("Scale parameter must be positive")
        return rng.exponential(scale, size=n)
    
    elif model == 'poisson':
        lam = params.get('lam', np.mean(data))
        if lam <= 0:
            raise ValueError("Lambda parameter must be positive")
        return rng.poisson(lam, size=n)
    
    else:
        raise ValueError(f"Unknown model: {model}. Use 'normal', 'exponential', or 'poisson'.")


def check_autocorrelation(data: np.ndarray, max_lag: int = 20) -> np.ndarray:
    """Compute autocorrelation function for a time series.
    
    Args:
        data: Time series data
        max_lag: Maximum lag to compute
        
    Returns:
        Array of autocorrelations from lag 0 to max_lag
    """
    n = len(data)
    if n < 2:
        raise ValueError("Need at least 2 observations")
    
    mean = np.mean(data)
    var = np.var(data, ddof=1)
    
    if var == 0:
        return np.ones(max_lag + 1)
    
    acf = np.zeros(max_lag + 1)
    for lag in range(max_lag + 1):
        if lag == 0:
            acf[lag] = 1.0
        else:
            if n - lag < 2:
                break
            cov = np.sum((data[:n-lag] - mean) * (data[lag:] - mean)) / (n - 1)
            acf[lag] = cov / var
    
    return acf


def estimate_block_size(data: np.ndarray, method: str = 'auto') -> int:
    """Estimate optimal block size for block bootstrap.
    
    Uses the method of Politis and White (2004) for automatic bandwidth selection.
    
    Args:
        data: Time series data
        method: Estimation method ('auto' or 'manual')
        
    Returns:
        Estimated block size
    """
    n = len(data)
    acf = check_autocorrelation(data, max_lag=min(n // 4, 100))
    
    # Find the first lag where ACF drops below 2/sqrt(n)
    threshold = 2 / np.sqrt(n)
    block_size = 1
    
    for lag in range(1, len(acf)):
        if abs(acf[lag]) < threshold:
            block_size = lag
            break
    else:
        block_size = len(acf) // 2
    
    # Ensure block size is at least 1 and at most n/4
    block_size = max(1, min(block_size, n // 4))
    
    return block_size


class ResamplingResult:
    """Base class for resampling test results."""
    
    def __init__(
        self,
        estimate: float,
        bootstrap_stats: Optional[np.ndarray] = None,
        std_error: Optional[float] = None,
        bias: Optional[float] = None,
        ci_lower: Optional[float] = None,
        ci_upper: Optional[float] = None,
        ci_level: Optional[float] = None,
        method: Optional[str] = None,
        n_resamples: Optional[int] = None,
        seed: Optional[int] = None
    ):
        self.estimate = estimate
        self.bootstrap_stats = bootstrap_stats
        self.std_error = std_error
        self.bias = bias
        self.ci_lower = ci_lower
        self.ci_upper = ci_upper
        self.ci_level = ci_level
        self.method = method
        self.n_resamples = n_resamples
        self.seed = seed
    
    def summary(self) -> str:
        """Return a summary string of the results."""
        lines = [f"Resampling Result"]
        lines.append(f"  Estimate: {self.estimate:.6f}")
        if self.std_error is not None:
            lines.append(f"  Std Error: {self.std_error:.6f}")
        if self.bias is not None:
            lines.append(f"  Bias: {self.bias:.6f}")
        if self.ci_lower is not None and self.ci_upper is not None:
            lines.append(f"  {self.ci_level*100:.1f}% CI [{self.ci_lower:.6f}, {self.ci_upper:.6f}]")
        if self.method is not None:
            lines.append(f"  Method: {self.method}")
        if self.n_resamples is not None:
            lines.append(f"  Resamples: {self.n_resamples}")
        return "\n".join(lines)
    
    def __repr__(self) -> str:
        return self.summary()
