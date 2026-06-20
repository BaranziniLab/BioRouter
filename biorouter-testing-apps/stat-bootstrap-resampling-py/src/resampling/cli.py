"""
Command-line interface for resampling toolkit.

Provides CLI commands for bootstrap, permutation tests, and jackknife.
"""

import argparse
import sys
import numpy as np
from typing import List, Optional

from .bootstrap import bootstrap, bootstrap_analysis
from .ci import bootstrap_ci
from .jackknife import jackknife
from .permutation import (
    two_sample_test,
    paired_test,
    correlation_test
)
from .block import block_bootstrap, block_bootstrap_ci


def parse_data(data_str: str) -> np.ndarray:
    """Parse comma-separated data string into numpy array."""
    try:
        values = [float(x.strip()) for x in data_str.split(',')]
        return np.array(values)
    except ValueError:
        print(f"Error: Could not parse data string: {data_str}", file=sys.stderr)
        sys.exit(1)


def cmd_bootstrap(args):
    """Handle bootstrap command."""
    data = parse_data(args.data)
    
    # Select statistic
    stat_func = _get_statistic(args.stat)
    
    # Perform bootstrap
    if args.ci:
        result = bootstrap_ci(
            data,
            stat_func,
            method=args.method,
            ci_level=args.level,
            B=args.B,
            seed=args.seed
        )
        print(result.summary())
    else:
        result = bootstrap_analysis(
            data,
            stat_func,
            method=args.method,
            B=args.B,
            seed=args.seed
        )
        print(result.summary())


def cmd_permutation(args):
    """Handle permutation test command."""
    sample1 = parse_data(args.group1)
    sample2 = parse_data(args.group2)
    
    # Perform test
    if args.test == 'paired':
        result = paired_test(
            sample1,
            sample2,
            alternative=args.alternative,
            B=args.B,
            seed=args.seed
        )
    elif args.test == 'correlation':
        result = correlation_test(
            sample1,
            sample2,
            alternative=args.alternative,
            B=args.B,
            seed=args.seed
        )
    else:
        result = two_sample_test(
            sample1,
            sample2,
            alternative=args.alternative,
            B=args.B,
            seed=args.seed
        )
    
    print(result.summary())
    
    # Interpretation
    alpha = args.alpha
    if result.p_value < alpha:
        print(f"\nResult is significant at α = {alpha}")
    else:
        print(f"\nResult is NOT significant at α = {alpha}")


def cmd_jackknife(args):
    """Handle jackknife command."""
    data = parse_data(args.data)
    
    # Select statistic
    stat_func = _get_statistic(args.stat)
    
    # Perform jackknife
    result = jackknife(data, stat_func, method=args.method)
    print(result.summary())


def cmd_block(args):
    """Handle block bootstrap command."""
    data = parse_data(args.data)
    
    # Select statistic
    stat_func = _get_statistic(args.stat)
    
    # Perform block bootstrap
    if args.ci:
        result = block_bootstrap_ci(
            data,
            stat_func,
            ci_level=args.level,
            method=args.method,
            block_size=args.block_size,
            B=args.B,
            seed=args.seed
        )
        print(result.summary())
    else:
        observed, boot_stats = block_bootstrap(
            data,
            stat_func,
            method=args.method,
            block_size=args.block_size,
            B=args.B,
            seed=args.seed
        )
        print(f"Observed: {observed:.6f}")
        print(f"Bootstrap SE: {np.std(boot_stats, ddof=1):.6f}")


def cmd_examples(args):
    """Run worked examples."""
    print("=" * 60)
    print("WORKED EXAMPLES")
    print("=" * 60)
    
    # Example 1: Bootstrap CI for mean
    print("\n1. Bootstrap CI for the Mean")
    print("-" * 40)
    np.random.seed(42)
    data = np.random.normal(loc=5, scale=2, size=100)
    print(f"Sample mean: {np.mean(data):.3f}")
    print(f"Sample std: {np.std(data, ddof=1):.3f}")
    print(f"Analytic SE: {np.std(data, ddof=1) / np.sqrt(len(data)):.3f}")
    
    result = bootstrap_ci(data, np.mean, method='percentile', B=9999, seed=42)
    print(f"Bootstrap SE: {result.std_error:.3f}")
    print(f"95% Percentile CI: [{result.ci_lower:.3f}, {result.ci_upper:.3f}]")
    
    result = bootstrap_ci(data, np.mean, method='bca', B=9999, seed=42)
    print(f"95% BCa CI: [{result.ci_lower:.3f}, {result.ci_upper:.3f}]")
    
    # Example 2: Bootstrap CI for median
    print("\n2. Bootstrap CI for the Median")
    print("-" * 40)
    print(f"Sample median: {np.median(data):.3f}")
    result = bootstrap_ci(data, np.median, method='percentile', B=9999, seed=42)
    print(f"95% CI: [{result.ci_lower:.3f}, {result.ci_upper:.3f}]")
    
    # Example 3: Bootstrap CI for correlation
    print("\n3. Bootstrap CI for Correlation")
    print("-" * 40)
    x = np.random.normal(0, 1, 100)
    y = 0.7 * x + np.random.normal(0, 0.5, 100)
    
    def corr_stat(data):
        return np.corrcoef(data[:len(data)//2], data[len(data)//2:])[0, 1]
    
    combined = np.concatenate([x, y])
    print(f"Sample correlation: {np.corrcoef(x, y)[0, 1]:.3f}")
    result = bootstrap_ci(combined, corr_stat, method='percentile', B=9999, seed=42)
    print(f"95% CI: [{result.ci_lower:.3f}, {result.ci_upper:.3f}]")
    
    # Example 4: Permutation test
    print("\n4. Permutation Test for Two Groups")
    print("-" * 40)
    np.random.seed(42)
    group_a = np.random.normal(loc=10, scale=2, size=50)
    group_b = np.random.normal(loc=12, scale=2, size=50)
    
    print(f"Group A mean: {np.mean(group_a):.3f}")
    print(f"Group B mean: {np.mean(group_b):.3f}")
    print(f"Observed difference: {np.mean(group_b) - np.mean(group_a):.3f}")
    
    result = two_sample_test(group_a, group_b, alternative='two-sided', B=9999, seed=42)
    print(f"P-value: {result.p_value:.4f}")
    print(f"Significant at α=0.05? {result.is_significant(0.05)}")
    
    # Example 5: Jackknife
    print("\n5. Jackknife Bias Estimation")
    print("-" * 40)
    
    def biased_mean(x):
        return np.mean(x) + 0.5  # Artificially biased estimator
    
    print(f"True mean: {np.mean(data):.3f}")
    print(f"Biased estimator: {biased_mean(data):.3f}")
    
    result = jackknife(data, biased_mean)
    print(f"Jackknife bias estimate: {result.bias:.3f}")
    print(f"Bias-corrected estimate: {result.bias_corrected:.3f}")
    
    # Example 6: Block bootstrap for time series
    print("\n6. Block Bootstrap for Time Series")
    print("-" * 40)
    np.random.seed(42)
    n = 200
    ts = np.zeros(n)
    ts[0] = 0
    for i in range(1, n):
        ts[i] = 0.5 * ts[i-1] + np.random.normal(0, 1)
    
    print(f"Time series length: {len(ts)}")
    print(f"Series mean: {np.mean(ts):.3f}")
    
    result = block_bootstrap_ci(ts, np.mean, method='moving', B=9999, seed=42)
    print(f"Block size: {result.block_size}")
    print(f"Bootstrap SE: {result.std_error:.3f}")
    print(f"95% CI: [{result.ci_lower:.3f}, {result.ci_upper:.3f}]")
    
    print("\n" + "=" * 60)
    print("Examples complete!")


def _get_statistic(stat_name: str):
    """Get statistic function by name."""
    stats = {
        'mean': np.mean,
        'median': np.median,
        'std': lambda x: np.std(x, ddof=1),
        'var': lambda x: np.var(x, ddof=1),
        'sum': np.sum,
        'min': np.min,
        'max': np.max,
    }
    
    if stat_name not in stats:
        print(f"Error: Unknown statistic '{stat_name}'", file=sys.stderr)
        print(f"Available: {', '.join(stats.keys())}", file=sys.stderr)
        sys.exit(1)
    
    return stats[stat_name]


def main():
    """Main entry point for CLI."""
    parser = argparse.ArgumentParser(
        description='Resampling Inference Toolkit',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  resampling bootstrap --data "1,2,3,4,5" --stat mean --method percentile
  resampling permutation --group1 "1,2,3" --group2 "4,5,6"
  resampling jackknife --data "1,2,3,4,5" --stat mean
  resampling block --data "1,2,3,4,5" --stat mean --method moving
  resampling examples
        """
    )
    
    subparsers = parser.add_subparsers(dest='command', help='Command to run')
    
    # Bootstrap command
    boot_parser = subparsers.add_parser('bootstrap', help='Bootstrap resampling')
    boot_parser.add_argument('--data', required=True, help='Comma-separated data')
    boot_parser.add_argument('--stat', default='mean', help='Statistic (mean, median, std, var)')
    boot_parser.add_argument('--method', default='nonparametric',
                           choices=['nonparametric', 'parametric', 'smoothed'],
                           help='Bootstrap method')
    boot_parser.add_argument('--B', type=int, default=9999, help='Number of resamples')
    boot_parser.add_argument('--seed', type=int, help='Random seed')
    boot_parser.add_argument('--ci', action='store_true', help='Compute confidence interval')
    boot_parser.add_argument('--level', type=float, default=0.95, help='CI level')
    boot_parser.set_defaults(func=cmd_bootstrap)
    
    # Permutation test command
    perm_parser = subparsers.add_parser('permutation', help='Permutation test')
    perm_parser.add_argument('--group1', required=True, help='First group (comma-separated)')
    perm_parser.add_argument('--group2', required=True, help='Second group (comma-separated)')
    perm_parser.add_argument('--test', default='two-sample',
                           choices=['two-sample', 'paired', 'correlation'],
                           help='Test type')
    perm_parser.add_argument('--alternative', default='two-sided',
                           choices=['two-sided', 'greater', 'less'],
                           help='Alternative hypothesis')
    perm_parser.add_argument('--B', type=int, default=9999, help='Number of permutations')
    perm_parser.add_argument('--seed', type=int, help='Random seed')
    perm_parser.add_argument('--alpha', type=float, default=0.05, help='Significance level')
    perm_parser.set_defaults(func=cmd_permutation)
    
    # Jackknife command
    jack_parser = subparsers.add_parser('jackknife', help='Jackknife resampling')
    jack_parser.add_argument('--data', required=True, help='Comma-separated data')
    jack_parser.add_argument('--stat', default='mean', help='Statistic')
    jack_parser.add_argument('--method', default='loo',
                           choices=['loo', 'delete-d'],
                           help='Jackknife method')
    jack_parser.set_defaults(func=cmd_jackknife)
    
    # Block bootstrap command
    block_parser = subparsers.add_parser('block', help='Block bootstrap')
    block_parser.add_argument('--data', required=True, help='Comma-separated data')
    block_parser.add_argument('--stat', default='mean', help='Statistic')
    block_parser.add_argument('--method', default='moving',
                            choices=['moving', 'stationary', 'circular'],
                            help='Block bootstrap method')
    block_parser.add_argument('--block-size', type=int, help='Block size')
    block_parser.add_argument('--B', type=int, default=9999, help='Number of resamples')
    block_parser.add_argument('--seed', type=int, help='Random seed')
    block_parser.add_argument('--ci', action='store_true', help='Compute confidence interval')
    block_parser.add_argument('--level', type=float, default=0.95, help='CI level')
    block_parser.set_defaults(func=cmd_block)
    
    # Examples command
    examples_parser = subparsers.add_parser('examples', help='Run worked examples')
    examples_parser.set_defaults(func=cmd_examples)
    
    args = parser.parse_args()
    
    if args.command is None:
        parser.print_help()
        sys.exit(0)
    
    args.func(args)


if __name__ == '__main__':
    main()
