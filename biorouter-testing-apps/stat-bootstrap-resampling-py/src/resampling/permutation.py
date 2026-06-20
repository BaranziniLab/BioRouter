"""
Permutation tests.

Implements two-sample difference test, correlation test, and paired test
with exact and Monte Carlo p-values.
"""

from typing import Callable, Optional, Tuple
from itertools import combinations
import numpy as np
from numpy.typing import ArrayLike

from .utils import (
    validate_data,
    validate_statistic,
    create_rng,
    ResamplingResult
)


def permutation_test(
    sample1: ArrayLike,
    sample2: ArrayLike,
    stat: Callable[[np.ndarray], float] = None,
    alternative: str = 'two-sided',
    B: int = 9999,
    seed: Optional[int] = None,
    rng: Optional[np.random.Generator] = None,
    exact: bool = False
) -> 'PermutationResult':
    """
    Perform a two-sample permutation test.
    
    Tests the null hypothesis that the two samples come from the same
    distribution.
    
    Args:
        sample1: First sample
        sample2: Second sample
        stat: Test statistic function (default: difference in means)
        alternative: 'two-sided', 'greater', or 'less'
        B: Number of permutations for Monte Carlo approximation
        seed: Random seed for reproducibility
        rng: Pre-initialized random generator
        exact: If True, enumerate all permutations (only for small samples)
        
    Returns:
        PermutationResult with p-value and test statistic
        
    Example:
        >>> group1 = np.random.normal(0, 1, 50)
        >>> group2 = np.random.normal(0.5, 1, 50)
        >>> result = permutation_test(group1, group2)
        >>> print(f"p-value: {result.p_value:.4f}")
    """
    sample1 = validate_data(sample1)
    sample2 = validate_data(sample2)
    rng = create_rng(seed, rng)
    
    # Default statistic: difference in means
    if stat is None:
        def stat(x):
            n1 = len(sample1)
            return np.mean(x[:n1]) - np.mean(x[n1:])
    
    # Compute observed statistic
    combined = np.concatenate([sample1, sample2])
    n1 = len(sample1)
    observed = stat(combined)
    
    if exact and (n1 + len(sample2)) <= 20:
        # Exact permutation test (enumerate all permutations)
        p_value = _exact_permutation_p(
            sample1, sample2, stat, alternative
        )
        n_permutations = _n_choose_k(len(combined), n1)
    else:
        # Monte Carlo permutation test
        p_value, n_permutations = _mc_permutation_p(
            combined, n1, stat, alternative, B, rng
        )
    
    return PermutationResult(
        test_statistic=observed,
        p_value=p_value,
        alternative=alternative,
        n_permutations=n_permutations,
        seed=seed,
        method='exact' if exact else 'monte_carlo',
        n_resamples=n_permutations
    )


def _exact_permutation_p(
    sample1: np.ndarray,
    sample2: np.ndarray,
    stat: Callable,
    alternative: str
) -> float:
    """
    Compute exact permutation p-value.
    
    Enumerates all possible permutations.
    
    Args:
        sample1: First sample
        sample2: Second sample
        stat: Test statistic function
        alternative: 'two-sided', 'greater', or 'less'
        
    Returns:
        Exact p-value
    """
    combined = np.concatenate([sample1, sample2])
    n = len(combined)
    n1 = len(sample1)
    
    # Compute observed statistic
    observed = stat(combined)
    
    # Enumerate all combinations of n1 indices
    count = 0
    total = 0
    
    for indices in combinations(range(n), n1):
        # Create permuted array
        perm = np.zeros(n, dtype=float)
        mask = np.zeros(n, dtype=bool)
        mask[list(indices)] = True
        
        perm[mask] = sample1
        perm[~mask] = sample2
        
        perm_stat = stat(perm)
        
        # Check if extreme
        if alternative == 'two-sided':
            if abs(perm_stat) >= abs(observed):
                count += 1
        elif alternative == 'greater':
            if perm_stat >= observed:
                count += 1
        elif alternative == 'less':
            if perm_stat <= observed:
                count += 1
        
        total += 1
    
    return count / total


def _mc_permutation_p(
    combined: np.ndarray,
    n1: int,
    stat: Callable,
    alternative: str,
    B: int,
    rng: np.random.Generator
) -> Tuple[float, int]:
    """
    Compute Monte Carlo permutation p-value.
    
    Args:
        combined: Combined samples
        n1: Size of first sample
        stat: Test statistic function
        alternative: 'two-sided', 'greater', or 'less'
        B: Number of permutations
        rng: Random number generator
        
    Returns:
        Tuple of (p-value, number of permutations)
    """
    # Compute observed statistic
    observed = stat(combined)
    
    # Count permutations at least as extreme
    count = 0
    n = len(combined)
    
    for _ in range(B):
        # Random permutation
        perm = combined.copy()
        rng.shuffle(perm)
        
        perm_stat = stat(perm)
        
        if alternative == 'two-sided':
            if abs(perm_stat) >= abs(observed):
                count += 1
        elif alternative == 'greater':
            if perm_stat >= observed:
                count += 1
        elif alternative == 'less':
            if perm_stat <= observed:
                count += 1
    
    # Add 1 for observed statistic
    p_value = (count + 1) / (B + 1)
    
    return p_value, B + 1


def _n_choose_k(n: int, k: int) -> int:
    """Compute binomial coefficient."""
    from math import comb
    return comb(n, k)


def two_sample_test(
    sample1: ArrayLike,
    sample2: ArrayLike,
    alternative: str = 'two-sided',
    B: int = 9999,
    seed: Optional[int] = None,
    exact: bool = False
) -> 'PermutationResult':
    """
    Two-sample permutation test for difference in means.
    
    Args:
        sample1: First sample
        sample2: Second sample
        alternative: 'two-sided', 'greater', or 'less'
        B: Number of permutations
        seed: Random seed
        exact: If True, enumerate all permutations
        
    Returns:
        PermutationResult
    """
    def diff_means(x):
        n1 = len(sample1)
        return np.mean(x[:n1]) - np.mean(x[n1:])
    
    return permutation_test(
        sample1, sample2, diff_means, alternative, B, seed, exact=exact
    )


def paired_test(
    sample1: ArrayLike,
    sample2: ArrayLike,
    alternative: str = 'two-sided',
    B: int = 9999,
    seed: Optional[int] = None
) -> 'PermutationResult':
    """
    Paired permutation test.
    
    Tests whether the distribution of differences is symmetric about zero.
    
    Args:
        sample1: First sample (paired)
        sample2: Second sample (paired)
        alternative: 'two-sided', 'greater', or 'less'
        B: Number of permutations
        seed: Random seed
        
    Returns:
        PermutationResult
    """
    sample1 = validate_data(sample1)
    sample2 = validate_data(sample2)
    rng = create_rng(seed)
    
    if len(sample1) != len(sample2):
        raise ValueError("Samples must have equal length for paired test")
    
    # Compute differences
    diffs = sample1 - sample2
    
    # Observed statistic: mean difference
    observed = np.mean(diffs)
    
    # Permutation test: flip signs of differences
    count = 0
    
    for _ in range(B):
        # Random sign flips
        signs = rng.choice([-1, 1], size=len(diffs))
        perm_diffs = signs * diffs
        perm_stat = np.mean(perm_diffs)
        
        if alternative == 'two-sided':
            if abs(perm_stat) >= abs(observed):
                count += 1
        elif alternative == 'greater':
            if perm_stat >= observed:
                count += 1
        elif alternative == 'less':
            if perm_stat <= observed:
                count += 1
    
    p_value = (count + 1) / (B + 1)
    
    return PermutationResult(
        test_statistic=observed,
        p_value=p_value,
        alternative=alternative,
        n_permutations=B + 1,
        seed=seed,
        method='monte_carlo',
        n_resamples=B + 1
    )


def correlation_test(
    x: ArrayLike,
    y: ArrayLike,
    alternative: str = 'two-sided',
    B: int = 9999,
    seed: Optional[int] = None
) -> 'PermutationResult':
    """
    Permutation test for correlation.
    
    Tests whether two variables are associated by permuting one variable.
    
    Args:
        x: First variable
        y: Second variable
        alternative: 'two-sided', 'greater', or 'less'
        B: Number of permutations
        seed: Random seed
        
    Returns:
        PermutationResult
    """
    x = validate_data(x)
    y = validate_data(y)
    rng = create_rng(seed)
    
    if len(x) != len(y):
        raise ValueError("x and y must have equal length")
    
    # Observed correlation
    observed = np.corrcoef(x, y)[0, 1]
    
    # Permutation test: permute y while keeping x fixed
    count = 0
    
    for _ in range(B):
        perm_y = rng.permutation(y)
        perm_corr = np.corrcoef(x, perm_y)[0, 1]
        
        if alternative == 'two-sided':
            if abs(perm_corr) >= abs(observed):
                count += 1
        elif alternative == 'greater':
            if perm_corr >= observed:
                count += 1
        elif alternative == 'less':
            if perm_corr <= observed:
                count += 1
    
    p_value = (count + 1) / (B + 1)
    
    return PermutationResult(
        test_statistic=observed,
        p_value=p_value,
        alternative=alternative,
        n_permutations=B + 1,
        seed=seed,
        method='monte_carlo',
        n_resamples=B + 1
    )


class PermutationResult(ResamplingResult):
    """Result of a permutation test."""
    
    def __init__(
        self,
        test_statistic: float,
        p_value: float,
        alternative: str = 'two-sided',
        n_permutations: int = 9999,
        seed: Optional[int] = None,
        method: str = 'monte_carlo',
        n_resamples: Optional[int] = None
    ):
        """
        Initialize PermutationResult.
        
        Args:
            test_statistic: Observed test statistic
            p_value: Permutation p-value
            alternative: Alternative hypothesis
            n_permutations: Number of permutations used
            seed: Random seed used
            method: Method used ('exact' or 'monte_carlo')
            n_resamples: Number of resamples
        """
        super().__init__(
            estimate=test_statistic,
            bootstrap_stats=None,
            std_error=None,
            bias=None,
            n_resamples=n_resamples or n_permutations,
            seed=seed,
            method=method
        )
        self.test_statistic = test_statistic
        self.p_value = p_value
        self.alternative = alternative
        self.n_permutations = n_permutations
    
    def summary(self) -> str:
        """Return a summary string of the permutation test results."""
        lines = ["Permutation Test Result"]
        lines.append(f"  Test Statistic: {self.test_statistic:.6f}")
        lines.append(f"  P-value: {self.p_value:.6f}")
        lines.append(f"  Alternative: {self.alternative}")
        lines.append(f"  Method: {self.method}")
        lines.append(f"  Permutations: {self.n_permutations}")
        return "\n".join(lines)
    
    def __repr__(self) -> str:
        return self.summary()
    
    def is_significant(self, alpha: float = 0.05) -> bool:
        """Check if result is significant at given alpha level."""
        return self.p_value < alpha
