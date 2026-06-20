"""
Resampling Inference Toolkit

A comprehensive Python library for bootstrap and resampling-based
statistical inference.

Modules:
    bootstrap: Nonparametric, parametric, and smoothed bootstrap
    ci: Confidence intervals (percentile, basic, BCa, bootstrap-t)
    jackknife: Leave-one-out and delete-d jackknife
    permutation: Permutation tests (two-sample, paired, correlation)
    block: Block bootstrap for dependent data
    cli: Command-line interface
"""

__version__ = "0.1.0"
__author__ = "Wanjun Gu"

# Import main API functions
from .bootstrap import (
    bootstrap,
    bootstrap_analysis,
    bootstrap_se,
    bootstrap_bias,
    nonparametric_bootstrap,
    parametric_bootstrap,
    smoothed_bootstrap,
    BootstrapResult,
)

from .ci import (
    percentile_ci,
    basic_ci,
    bca_ci,
    bootstrap_t_ci,
    bootstrap_ci,
    CIResult,
)

from .jackknife import (
    jackknife,
    jackknife_variance,
    jackknife_bias,
    jackknife_ci,
    JackknifeResult,
)

from .permutation import (
    permutation_test,
    two_sample_test,
    paired_test,
    correlation_test,
    PermutationResult,
)

from .block import (
    block_bootstrap,
    block_bootstrap_ci,
    moving_block_bootstrap,
    stationary_block_bootstrap,
    circular_block_bootstrap,
    BlockBootstrapResult,
)

# Public API
__all__ = [
    # Bootstrap
    'bootstrap',
    'bootstrap_analysis',
    'bootstrap_se',
    'bootstrap_bias',
    'nonparametric_bootstrap',
    'parametric_bootstrap',
    'smoothed_bootstrap',
    'BootstrapResult',
    
    # Confidence Intervals
    'percentile_ci',
    'basic_ci',
    'bca_ci',
    'bootstrap_t_ci',
    'CIResult',
    
    # Jackknife
    'jackknife',
    'jackknife_variance',
    'jackknife_bias',
    'jackknife_ci',
    'JackknifeResult',
    
    # Permutation Tests
    'permutation_test',
    'two_sample_test',
    'paired_test',
    'correlation_test',
    'PermutationResult',
    
    # Block Bootstrap
    'block_bootstrap',
    'block_bootstrap_ci',
    'moving_block_bootstrap',
    'stationary_block_bootstrap',
    'circular_block_bootstrap',
    'BlockBootstrapResult',
]
