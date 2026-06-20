"""
Block bootstrap methods for dependent data.

Implements moving block bootstrap and stationary block bootstrap
for time series and spatial data.
"""

from typing import Callable, Optional, Tuple
import numpy as np
from numpy.typing import ArrayLike

from .utils import (
    validate_data,
    validate_statistic,
    create_rng,
    compute_std_error,
    estimate_block_size,
    check_autocorrelation,
    ResamplingResult
)


def moving_block_bootstrap(
    data: ArrayLike,
    stat: Callable[[np.ndarray], float],
    block_size: Optional[int] = None,
    B: int = 9999,
    seed: Optional[int] = None,
    rng: Optional[np.random.Generator] = None
) -> Tuple[float, np.ndarray]:
    """
    Perform moving block bootstrap (MBB).
    
    The MBB samples blocks of consecutive observations with replacement,
    maintaining local dependence structure.
    
    Args:
        data: 1D array of observations (e.g., time series)
        stat: Statistic function
        block_size: Size of blocks (default: auto-estimated)
        B: Number of bootstrap resamples
        seed: Random seed for reproducibility
        rng: Pre-initialized random generator
        
    Returns:
        Tuple of (observed statistic, array of B bootstrap statistics)
        
    Example:
        >>> ts = np.cumsum(np.random.normal(0, 1, 100))
        >>> obs, boot_stats = moving_block_bootstrap(ts, np.mean, B=999)
    """
    data = validate_data(data)
    stat = validate_statistic(stat)
    rng = create_rng(seed, rng)
    n = len(data)
    
    # Auto-estimate block size if not provided
    if block_size is None:
        block_size = estimate_block_size(data)
    
    # Ensure block size is reasonable
    block_size = max(1, min(block_size, n // 2))
    
    # Compute observed statistic
    observed = stat(data)
    
    # Number of blocks needed to get approximately n observations
    n_blocks = int(np.ceil(n / block_size))
    
    # Bootstrap resamples
    boot_stats = np.zeros(B)
    for b in range(B):
        # Sample starting indices for blocks
        max_start = n - block_size
        starts = rng.integers(0, max_start + 1, size=n_blocks)
        
        # Concatenate blocks
        blocks = [data[start:start + block_size] for start in starts]
        boot_sample = np.concatenate(blocks)[:n]  # Truncate to original length
        
        boot_stats[b] = stat(boot_sample)
    
    return observed, boot_stats


def stationary_block_bootstrap(
    data: ArrayLike,
    stat: Callable[[np.ndarray], float],
    block_size: Optional[int] = None,
    B: int = 9999,
    seed: Optional[int] = None,
    rng: np.random.Generator = None
) -> Tuple[float, np.ndarray]:
    """
    Perform stationary block bootstrap (SBB).
    
    The SBB uses geometrically distributed block sizes, which is more
    appropriate for data with long-range dependence.
    
    Args:
        data: 1D array of observations
        stat: Statistic function
        block_size: Mean block size (default: auto-estimated)
        B: Number of bootstrap resamples
        seed: Random seed for reproducibility
        rng: Pre-initialized random generator
        
    Returns:
        Tuple of (observed statistic, array of B bootstrap statistics)
        
    References:
        Politis, D. N., & Romano, J. P. (1994). The stationary bootstrap.
    """
    data = validate_data(data)
    stat = validate_statistic(stat)
    rng = create_rng(seed, rng)
    n = len(data)
    
    # Auto-estimate mean block size if not provided
    if block_size is None:
        block_size = estimate_block_size(data)
    
    # Block size is the mean of geometric distribution
    p = 1.0 / block_size  # Probability of starting a new block
    
    # Compute observed statistic
    observed = stat(data)
    
    # Bootstrap resamples
    boot_stats = np.zeros(B)
    for b in range(B):
        boot_sample = np.zeros(n)
        
        # Random starting position
        pos = rng.integers(0, n)
        block_len = 0
        
        for i in range(n):
            # Check if we should start a new block
            if block_len == 0 or rng.random() < p:
                pos = rng.integers(0, n)
                block_len = 1
            else:
                block_len += 1
            
            # Sample from current position (wrap around)
            boot_sample[i] = data[pos % n]
            pos = (pos + 1) % n
        
        boot_stats[b] = stat(boot_sample)
    
    return observed, boot_stats


def circular_block_bootstrap(
    data: ArrayLike,
    stat: Callable[[np.ndarray], float],
    block_size: Optional[int] = None,
    B: int = 9999,
    seed: Optional[int] = None,
    rng: np.random.Generator = None
) -> Tuple[float, np.ndarray]:
    """
    Perform circular block bootstrap.
    
    Similar to MBB but wraps around circularly, ensuring the bootstrap
    sample has exactly the same length as the original.
    
    Args:
        data: 1D array of observations
        stat: Statistic function
        block_size: Size of blocks (default: auto-estimated)
        B: Number of bootstrap resamples
        seed: Random seed for reproducibility
        rng: Pre-initialized random generator
        
    Returns:
        Tuple of (observed statistic, array of B bootstrap statistics)
    """
    data = validate_data(data)
    stat = validate_statistic(stat)
    rng = create_rng(seed, rng)
    n = len(data)
    
    # Auto-estimate block size if not provided
    if block_size is None:
        block_size = estimate_block_size(data)
    
    # Ensure block size is reasonable
    block_size = max(1, min(block_size, n // 2))
    
    # Compute observed statistic
    observed = stat(data)
    
    # Number of blocks
    n_blocks = int(np.ceil(n / block_size))
    
    # Bootstrap resamples
    boot_stats = np.zeros(B)
    for b in range(B):
        boot_sample = np.zeros(n)
        
        # Sample starting indices
        starts = rng.integers(0, n, size=n_blocks)
        
        # Fill sample with blocks (circular wrap-around)
        idx = 0
        for start in starts:
            for j in range(block_size):
                if idx >= n:
                    break
                boot_sample[idx] = data[(start + j) % n]
                idx += 1
            if idx >= n:
                break
        
        boot_stats[b] = stat(boot_sample)
    
    return observed, boot_stats


def block_bootstrap(
    data: ArrayLike,
    stat: Callable[[np.ndarray], float],
    method: str = 'moving',
    block_size: Optional[int] = None,
    B: int = 9999,
    seed: Optional[int] = None,
    rng: np.random.Generator = None
) -> Tuple[float, np.ndarray]:
    """
    General block bootstrap dispatcher.
    
    Args:
        data: 1D array of observations
        stat: Statistic function
        method: 'moving', 'stationary', or 'circular'
        block_size: Size of blocks (default: auto-estimated)
        B: Number of bootstrap resamples
        seed: Random seed
        rng: Pre-initialized random generator
        
    Returns:
        Tuple of (observed statistic, array of B bootstrap statistics)
    """
    if method == 'moving':
        return moving_block_bootstrap(data, stat, block_size, B, seed, rng)
    elif method == 'stationary':
        return stationary_block_bootstrap(data, stat, block_size, B, seed, rng)
    elif method == 'circular':
        return circular_block_bootstrap(data, stat, block_size, B, seed, rng)
    else:
        raise ValueError(
            f"Unknown method: {method}. Use 'moving', 'stationary', or 'circular'."
        )


def block_bootstrap_ci(
    data: ArrayLike,
    stat: Callable[[np.ndarray], float],
    ci_level: float = 0.95,
    method: str = 'moving',
    block_size: Optional[int] = None,
    B: int = 9999,
    seed: Optional[int] = None
) -> 'BlockBootstrapResult':
    """
    Perform block bootstrap with confidence interval.
    
    Args:
        data: 1D array of observations
        stat: Statistic function
        ci_level: Confidence level
        method: Block bootstrap method
        block_size: Block size
        B: Number of resamples
        seed: Random seed
        
    Returns:
        BlockBootstrapResult with CI and diagnostics
    """
    from .ci import percentile_ci
    
    data = validate_data(data)
    stat = validate_statistic(stat)
    rng = create_rng(seed)
    
    observed, boot_stats = block_bootstrap(
        data, stat, method, block_size, B, seed=seed
    )
    
    # Compute percentile CI
    ci_lower, ci_upper = percentile_ci(boot_stats, ci_level)
    
    return BlockBootstrapResult(
        estimate=observed,
        bootstrap_stats=boot_stats,
        ci_lower=ci_lower,
        ci_upper=ci_upper,
        ci_level=ci_level,
        method=method,
        block_size=block_size or estimate_block_size(data),
        n_resamples=B,
        seed=seed,
        std_error=compute_std_error(boot_stats),
        bias=float(np.mean(boot_stats) - observed)
    )


class BlockBootstrapResult(ResamplingResult):
    """Result of a block bootstrap analysis."""
    
    def __init__(
        self,
        estimate: float,
        bootstrap_stats: np.ndarray,
        ci_lower: float,
        ci_upper: float,
        ci_level: float = 0.95,
        method: str = 'moving',
        block_size: int = 10,
        n_resamples: int = 9999,
        seed: Optional[int] = None,
        std_error: Optional[float] = None,
        bias: Optional[float] = None
    ):
        """
        Initialize BlockBootstrapResult.
        
        Args:
            estimate: Observed statistic
            bootstrap_stats: Array of bootstrap statistics
            ci_lower: Lower CI bound
            ci_upper: Upper CI bound
            ci_level: Confidence level
            method: Block bootstrap method
            block_size: Block size used
            n_resamples: Number of resamples
            seed: Random seed
            std_error: Bootstrap SE
            bias: Bootstrap bias
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
        self.block_size = block_size
    
    def summary(self) -> str:
        """Return a summary string of the block bootstrap results."""
        lines = ["Block Bootstrap Result"]
        lines.append(f"  Estimate: {self.estimate:.6f}")
        lines.append(f"  Std Error: {self.std_error:.6f}")
        lines.append(f"  Bias: {self.bias:.6f}")
        lines.append(f"  {self.ci_level*100:.1f}% CI [{self.ci_lower:.6f}, {self.ci_upper:.6f}]")
        lines.append(f"  Method: {self.method}")
        lines.append(f"  Block Size: {self.block_size}")
        lines.append(f"  Resamples: {self.n_resamples}")
        return "\n".join(lines)
    
    def __repr__(self) -> str:
        return self.summary()
