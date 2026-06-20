"""
Worked examples demonstrating the resampling toolkit.

This script shows practical usage of the main features:
- Bootstrap confidence intervals
- Permutation tests
- Jackknife
- Block bootstrap for time series
"""

import numpy as np

# Add parent directory to path for imports
import sys
sys.path.insert(0, '../src')

from resampling import (
    bootstrap_ci,
    bootstrap_se,
    bootstrap_analysis,
    two_sample_test,
    paired_test,
    correlation_test,
    jackknife,
    jackknife_ci,
    block_bootstrap_ci,
)


def example_bootstrap_ci_mean():
    """Example 1: Bootstrap CI for the mean."""
    print("=" * 60)
    print("Example 1: Bootstrap Confidence Interval for the Mean")
    print("=" * 60)
    
    # Generate data
    np.random.seed(42)
    data = np.random.normal(loc=5, scale=2, size=100)
    
    print(f"\nSample size: {len(data)}")
    print(f"Sample mean: {np.mean(data):.3f}")
    print(f"Sample std: {np.std(data, ddof=1):.3f}")
    
    # Analytic SE for comparison
    analytic_se = np.std(data, ddof=1) / np.sqrt(len(data))
    print(f"Analytic SE: {analytic_se:.3f}")
    
    # Bootstrap SE
    boot_se = bootstrap_se(data, np.mean, B=9999, seed=42)
    print(f"Bootstrap SE: {boot_se:.3f}")
    
    # Different CI methods
    print("\n95% Confidence Intervals:")
    
    # Percentile
    result = bootstrap_ci(data, np.mean, method='percentile', B=9999, seed=42)
    print(f"  Percentile: [{result.ci_lower:.3f}, {result.ci_upper:.3f}]")
    
    # BCa
    result = bootstrap_ci(data, np.mean, method='bca', B=9999, seed=42)
    print(f"  BCa:        [{result.ci_lower:.3f}, {result.ci_upper:.3f}]")
    
    # Basic
    result = bootstrap_ci(data, np.mean, method='basic', B=9999, seed=42)
    print(f"  Basic:      [{result.ci_lower:.3f}, {result.ci_upper:.3f}]")
    
    # Bootstrap-t
    result = bootstrap_ci(data, np.mean, method='bootstrap_t', B=9999, seed=42)
    print(f"  Bootstrap-t:[{result.ci_lower:.3f}, {result.ci_upper:.3f}]")


def example_bootstrap_ci_median():
    """Example 2: Bootstrap CI for the median."""
    print("\n" + "=" * 60)
    print("Example 2: Bootstrap Confidence Interval for the Median")
    print("=" * 60)
    
    np.random.seed(42)
    data = np.random.normal(loc=5, scale=2, size=100)
    
    print(f"\nSample median: {np.median(data):.3f}")
    
    # BCa CI for median (median doesn't have analytic SE)
    result = bootstrap_ci(data, np.median, method='bca', B=9999, seed=42)
    print(f"95% BCa CI: [{result.ci_lower:.3f}, {result.ci_upper:.3f}]")
    print(f"Bootstrap SE: {result.std_error:.3f}")


def example_bootstrap_ci_correlation():
    """Example 3: Bootstrap CI for correlation."""
    print("\n" + "=" * 60)
    print("Example 3: Bootstrap Confidence Interval for Correlation")
    print("=" * 60)
    
    np.random.seed(42)
    n = 100
    x = np.random.normal(0, 1, n)
    y = 0.7 * x + np.random.normal(0, 0.5, n)
    
    obs_corr = np.corrcoef(x, y)[0, 1]
    print(f"\nSample correlation: {obs_corr:.3f}")
    
    # Define statistic function for correlation
    def corr_stat(data):
        half = len(data) // 2
        return np.corrcoef(data[:half], data[half:])[0, 1]
    
    # Bootstrap CI
    combined = np.concatenate([x, y])
    result = bootstrap_ci(combined, corr_stat, method='bca', B=9999, seed=42)
    print(f"95% BCa CI: [{result.ci_lower:.3f}, {result.ci_upper:.3f}]")
    print(f"Bootstrap SE: {result.std_error:.3f}")


def example_permutation_test():
    """Example 4: Permutation test for two groups."""
    print("\n" + "=" * 60)
    print("Example 4: Permutation Test for Two Groups")
    print("=" * 60)
    
    np.random.seed(42)
    group_a = np.random.normal(loc=10, scale=2, size=50)
    group_b = np.random.normal(loc=12, scale=2, size=50)
    
    print(f"\nGroup A: n={len(group_a)}, mean={np.mean(group_a):.3f}, std={np.std(group_a, ddof=1):.3f}")
    print(f"Group B: n={len(group_b)}, mean={np.mean(group_b):.3f}, std={np.std(group_b, ddof=1):.3f}")
    print(f"Observed difference: {np.mean(group_b) - np.mean(group_a):.3f}")
    
    # Permutation test
    result = two_sample_test(group_a, group_b, alternative='two-sided', B=9999, seed=42)
    
    print(f"\nPermutation test result:")
    print(f"  Test statistic: {result.test_statistic:.3f}")
    print(f"  P-value: {result.p_value:.4f}")
    print(f"  Significant at α=0.05? {result.is_significant(0.05)}")


def example_paired_test():
    """Example 5: Paired permutation test."""
    print("\n" + "=" * 60)
    print("Example 5: Paired Permutation Test")
    print("=" * 60)
    
    np.random.seed(42)
    n = 50
    
    # Pre/post intervention data (paired)
    pre = np.random.normal(loc=100, scale=15, size=n)
    post = pre + np.random.normal(loc=-5, scale=10, size=n)  # Intervention effect
    
    print(f"\nPre-intervention: mean={np.mean(pre):.3f}, std={np.std(pre, ddof=1):.3f}")
    print(f"Post-intervention: mean={np.mean(post):.3f}, std={np.std(post, ddof=1):.3f}")
    print(f"Mean difference: {np.mean(post - pre):.3f}")
    
    # Paired test
    result = paired_test(pre, post, alternative='two-sided', B=9999, seed=42)
    
    print(f"\nPaired test result:")
    print(f"  Test statistic: {result.test_statistic:.3f}")
    print(f"  P-value: {result.p_value:.4f}")
    print(f"  Significant at α=0.05? {result.is_significant(0.05)}")


def example_correlation_test():
    """Example 6: Correlation permutation test."""
    print("\n" + "=" * 60)
    print("Example 6: Correlation Permutation Test")
    print("=" * 60)
    
    np.random.seed(42)
    n = 100
    x = np.random.normal(0, 1, n)
    y = 0.6 * x + np.random.normal(0, 0.8, n)
    
    print(f"\nX: mean={np.mean(x):.3f}, std={np.std(x, ddof=1):.3f}")
    print(f"Y: mean={np.mean(y):.3f}, std={np.std(y, ddof=1):.3f}")
    print(f"Sample correlation: {np.corrcoef(x, y)[0, 1]:.3f}")
    
    # Correlation test
    result = correlation_test(x, y, alternative='two-sided', B=9999, seed=42)
    
    print(f"\nCorrelation test result:")
    print(f"  Test statistic: {result.test_statistic:.3f}")
    print(f"  P-value: {result.p_value:.4f}")
    print(f"  Significant at α=0.05? {result.is_significant(0.05)}")


def example_jackknife():
    """Example 7: Jackknife bias estimation."""
    print("\n" + "=" * 60)
    print("Example 7: Jackknife Bias Estimation")
    print("=" * 60)
    
    np.random.seed(42)
    data = np.random.normal(loc=5, scale=2, size=100)
    
    # True mean
    true_mean = 5.0
    sample_mean = np.mean(data)
    
    print(f"\nTrue mean: {true_mean:.3f}")
    print(f"Sample mean: {sample_mean:.3f}")
    
    # Biased estimator
    def biased_estimator(x):
        return np.mean(x) + 1.0  # Always overestimates by 1
    
    biased_est = biased_estimator(data)
    print(f"Biased estimator: {biased_est:.3f}")
    
    # Jackknife analysis
    result = jackknife(data, biased_estimator)
    
    print(f"\nJackknife results:")
    print(f"  Bias estimate: {result.bias:.3f}")
    print(f"  Bias-corrected estimate: {result.bias_corrected:.3f}")
    print(f"  Standard error: {result.std_error:.3f}")
    
    # Jackknife CI
    lower, upper = jackknife_ci(data, sample_mean.__class__.__call__, ci_level=0.95)
    # Using mean for CI example
    lower, upper = jackknife_ci(data, np.mean, ci_level=0.95)
    print(f"\n95% Jackknife CI for mean: [{lower:.3f}, {upper:.3f}]")


def example_block_bootstrap():
    """Example 8: Block bootstrap for time series."""
    print("\n" + "=" * 60)
    print("Example 8: Block Bootstrap for Time Series")
    print("=" * 60)
    
    np.random.seed(42)
    n = 200
    
    # Generate AR(1) process (autocorrelated)
    ts = np.zeros(n)
    ts[0] = 0
    for i in range(1, n):
        ts[i] = 0.5 * ts[i-1] + np.random.normal(0, 1)
    
    print(f"\nTime series length: {len(ts)}")
    print(f"Series mean: {np.mean(ts):.3f}")
    print(f"Series std: {np.std(ts, ddof=1):.3f}")
    
    # Block bootstrap
    result = block_bootstrap_ci(ts, np.mean, method='moving', B=9999, seed=42)
    
    print(f"\nBlock bootstrap results:")
    print(f"  Block size: {result.block_size}")
    print(f"  Bootstrap SE: {result.std_error:.3f}")
    print(f"  95% CI: [{result.ci_lower:.3f}, {result.ci_upper:.3f}]")
    
    # Compare with naive SE
    naive_se = np.std(ts, ddof=1) / np.sqrt(n)
    print(f"\n  Naive SE (incorrect for autocorrelated data): {naive_se:.3f}")
    print(f"  Block bootstrap SE (better): {result.std_error:.3f}")


def run_all_examples():
    """Run all worked examples."""
    print("\n" + "=" * 60)
    print("RESAMPLING INFERENCE TOOLKIT - WORKED EXAMPLES")
    print("=" * 60)
    
    example_bootstrap_ci_mean()
    example_bootstrap_ci_median()
    example_bootstrap_ci_correlation()
    example_permutation_test()
    example_paired_test()
    example_correlation_test()
    example_jackknife()
    example_block_bootstrap()
    
    print("\n" + "=" * 60)
    print("All examples completed successfully!")
    print("=" * 60)


if __name__ == '__main__':
    run_all_examples()
