"""
Bootstrap resampling methods.

Implements nonparametric (case), parametric, smoothed, and block bootstrap
for statistical inference.
"""

from typing import Any, Callable, Optional, Tuple, Union
import numpy as np
from numpy.typing import ArrayLike

from .utils import (
    validate_data,
    validate_statistic,
    create_rng,
    compute_std_error,
    ResamplingResult
)


def nonparametric_bootstrap(
    data: ArrayLike,
    stat: Callable[[np.ndarray], float],
    B: int = 9999,
    seed: Optional[int] = None,
    rng: Optional[np.random.Generator] = None
) -> Tuple[float, np.ndarray]:
    """
    Perform nonparametric (case) bootstrap.
    
    Resamples observations with replacement from the original data.
    
    Args:
        data: 1D array of observations
        stat: Statistic function (takes array, returns scalar)
        B: Number of bootstrap resamples
        seed: Random seed for reproducibility
        rng: Pre-initialized random generator (overrides seed)
        
    Returns:
        Tuple of (observed statistic, array of B bootstrap statistics)
        
    Example:
        >>> data = np.random.normal(0, 1, 100)
        >>> obs, boot_stats = nonparametric_bootstrap(data, np.mean, B=999)
        >>> print(f"Observed: {obs}, Bootstrap SE: {np.std(boot_stats):.3f}")
    """
    data = validate_data(data)
    stat = validate_statistic(stat)
    rng = create_rng(seed, rng)
    n = len(data)
    
    # Compute observed statistic
    observed = stat(data)
    
    # Bootstrap resamples
    boot_stats = np.zeros(B)
    for b in range(B):
        # Sample indices with replacement
        indices = rng.integers(0, n, size=n)
        boot_sample = data[indices]
        boot_stats[b] = stat(boot_sample)
    
    return observed, boot_stats


def parametric_bootstrap(
    data: ArrayLike,
    stat: Callable[[np.ndarray], float],
    B: int = 9999,
    model: str = 'normal',
    seed: Optional[int] = None,
    rng: Optional[np.random.Generator] = None,
    **params
) -> Tuple[float, np.ndarray]:
    """
    Perform parametric bootstrap.
    
    Fits a parametric model to the data, then resamples from the fitted
    distribution.
    
    Args:
        data: 1D array of observations
        stat: Statistic function (takes array, returns scalar)
        B: Number of bootstrap resamples
        model: Distribution type ('normal', 'exponential', 'poisson')
        seed: Random seed for reproducibility
        rng: Pre-initialized random generator
        **params: Distribution parameters (if not provided, fitted from data)
        
    Returns:
        Tuple of (observed statistic, array of B bootstrap statistics)
        
    Example:
        >>> data = np.random.exponential(2, 100)
        >>> obs, boot_stats = parametric_bootstrap(data, np.mean, B=999, model='exponential')
    """
    data = validate_data(data)
    stat = validate_statistic(stat)
    rng = create_rng(seed, rng)
    n = len(data)
    
    # Compute observed statistic
    observed = stat(data)
    
    # Fit parameters if not provided
    if model == 'normal':
        mu = params.get('mu', np.mean(data))
        sigma = params.get('sigma', np.std(data, ddof=1))
        fitted_params = {'mu': mu, 'sigma': sigma}
    elif model == 'exponential':
        scale = params.get('scale', np.mean(data))
        fitted_params = {'scale': scale}
    elif model == 'poisson':
        lam = params.get('lam', np.mean(data))
        fitted_params = {'lam': lam}
    else:
        raise ValueError(f"Unknown model: {model}")
    
    # Bootstrap resamples from fitted distribution
    boot_stats = np.zeros(B)
    for b in range(B):
        if model == 'normal':
            boot_sample = rng.normal(fitted_params['mu'], fitted_params['sigma'], size=n)
        elif model == 'exponential':
            boot_sample = rng.exponential(fitted_params['scale'], size=n)
        elif model == 'poisson':
            boot_sample = rng.poisson(fitted_params['lam'], size=n)
        
        boot_stats[b] = stat(boot_sample)
    
    return observed, boot_stats


def smoothed_bootstrap(
    data: ArrayLike,
    stat: Callable[[np.ndarray], float],
    B: int = 9999,
    bandwidth: Optional[float] = None,
    seed: Optional[int] = None,
    rng: Optional[np.random.Generator] = None
) -> Tuple[float, np.ndarray]:
    """
    Perform smoothed bootstrap.
    
    Adds kernel noise to bootstrap samples for smoother density estimation.
    Uses Gaussian kernel by default.
    
    Args:
        data: 1D array of observations
        stat: Statistic function
        B: Number of bootstrap resamples
        bandwidth: Kernel bandwidth (default: Silverman's rule of thumb)
        seed: Random seed for reproducibility
        rng: Pre-initialized random generator
        
    Returns:
        Tuple of (observed statistic, array of B bootstrap statistics)
        
    Example:
        >>> data = np.random.normal(0, 1, 100)
        >>> obs, boot_stats = smoothed_bootstrap(data, np.mean, B=999)
    """
    data = validate_data(data)
    stat = validate_statistic(stat)
    rng = create_rng(seed, rng)
    n = len(data)
    
    # Compute observed statistic
    observed = stat(data)
    
    # Default bandwidth: Silverman's rule of thumb
    if bandwidth is None:
        bandwidth = np.std(data, ddof=1) * n ** (-1/5)
    
    # Bootstrap resamples with smoothing
    boot_stats = np.zeros(B)
    for b in range(B):
        # Sample indices with replacement
        indices = rng.integers(0, n, size=n)
        boot_sample = data[indices]
        
        # Add kernel noise
        noise = rng.normal(0, bandwidth, size=n)
        smoothed_sample = boot_sample + noise
        
        boot_stats[b] = stat(smoothed_sample)
    
    return observed, boot_stats


def bootstrap(
    data: ArrayLike,
    stat: Callable[[np.ndarray], float],
    method: str = 'nonparametric',
    B: int = 9999,
    seed: Optional[int] = None,
    rng: Optional[np.random.Generator] = None,
    **kwargs
) -> Tuple[float, np.ndarray]:
    """
    General bootstrap dispatcher.
    
    Args:
        data: 1D array of observations
        stat: Statistic function
        method: Bootstrap method ('nonparametric', 'parametric', 'smoothed')
        B: Number of bootstrap resamples
        seed: Random seed for reproducibility
        rng: Pre-initialized random generator
        **kwargs: Additional arguments for specific methods
        
    Returns:
        Tuple of (observed statistic, array of B bootstrap statistics)
    """
    if method == 'nonparametric':
        return nonparametric_bootstrap(data, stat, B, seed, rng)
    elif method == 'parametric':
        return parametric_bootstrap(data, stat, B, seed=seed, rng=rng, **kwargs)
    elif method == 'smoothed':
        bandwidth = kwargs.get('bandwidth', None)
        return smoothed_bootstrap(data, stat, B, bandwidth, seed, rng)
    else:
        raise ValueError(
            f"Unknown method: {method}. Use 'nonparametric', 'parametric', or 'smoothed'."
        )


def bootstrap_se(
    data: ArrayLike,
    stat: Callable[[np.ndarray], float],
    B: int = 9999,
    method: str = 'nonparametric',
    seed: Optional[int] = None,
    **kwargs
) -> float:
    """
    Compute bootstrap standard error.
    
    Args:
        data: 1D array of observations
        stat: Statistic function
        B: Number of bootstrap resamples
        method: Bootstrap method
        seed: Random seed
        **kwargs: Additional arguments for specific methods
        
    Returns:
        Bootstrap standard error
    """
    _, boot_stats = bootstrap(data, stat, method, B, seed, **kwargs)
    return compute_std_error(boot_stats)


def bootstrap_bias(
    data: ArrayLike,
    stat: Callable[[np.ndarray], float],
    B: int = 9999,
    method: str = 'nonparametric',
    seed: Optional[int] = None,
    **kwargs
) -> Tuple[float, float]:
    """
    Compute bootstrap bias estimate.
    
    The bootstrap bias is estimated as:
        bias* = mean(T*) - T
    
    where T is the observed statistic and T* are bootstrap replicates.
    
    Args:
        data: 1D array of observations
        stat: Statistic function
        B: Number of bootstrap resamples
        method: Bootstrap method
        seed: Random seed
        **kwargs: Additional arguments
        
    Returns:
        Tuple of (observed statistic, estimated bias)
    """
    observed, boot_stats = bootstrap(data, stat, method, B, seed, **kwargs)
    bias = np.mean(boot_stats) - observed
    return observed, bias


class BootstrapResult(ResamplingResult):
    """Result of a bootstrap analysis."""
    
    def __init__(
        self,
        estimate: float,
        bootstrap_stats: np.ndarray,
        method: str = 'nonparametric',
        seed: Optional[int] = None
    ):
        """
        Initialize BootstrapResult.
        
        Args:
            estimate: Observed statistic
            bootstrap_stats: Array of bootstrap statistics
            method: Bootstrap method used
            seed: Random seed used
        """
        super().__init__(
            estimate=estimate,
            bootstrap_stats=bootstrap_stats,
            std_error=compute_std_error(bootstrap_stats),
            bias=float(np.mean(bootstrap_stats) - estimate),
            n_resamples=len(bootstrap_stats),
            method=method,
            seed=seed
        )
    
    def convergence_plot_data(self) -> dict:
        """
        Get data for convergence analysis.
        
        Returns statistics computed on first k resamples for k = 10, 20, ..., B.
        
        Returns:
            Dictionary with 'k_values' and 'se_values'
        """
        B = len(self.bootstrap_stats)
        k_values = []
        se_values = []
        
        for k in range(10, B + 1, max(1, B // 50)):
            se = np.std(self.bootstrap_stats[:k], ddof=1)
            k_values.append(k)
            se_values.append(se)
        
        return {'k_values': k_values, 'se_values': se_values}


def bootstrap_analysis(
    data: ArrayLike,
    stat: Callable[[np.ndarray], float],
    method: str = 'nonparametric',
    B: int = 9999,
    seed: Optional[int] = None,
    **kwargs
) -> BootstrapResult:
    """
    Complete bootstrap analysis with result object.
    
    Args:
        data: 1D array of observations
        stat: Statistic function
        method: Bootstrap method
        B: Number of bootstrap resamples
        seed: Random seed
        **kwargs: Additional arguments
        
    Returns:
        BootstrapResult object with all statistics
    """
    data = validate_data(data)
    stat = validate_statistic(stat)
    
    observed, boot_stats = bootstrap(data, stat, method, B, seed, **kwargs)
    
    return BootstrapResult(
        estimate=observed,
        bootstrap_stats=boot_stats,
        method=method,
        seed=seed
    )
