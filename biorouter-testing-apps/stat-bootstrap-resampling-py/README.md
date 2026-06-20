# stat-bootstrap-resampling-py

A comprehensive Python toolkit for bootstrap and resampling-based statistical inference.

## Features

### Bootstrap Methods
- **Nonparametric (Case) Bootstrap**: Resample observations with replacement
- **Parametric Bootstrap**: Fit a model, resample from the fitted distribution
- **Smoothed Bootstrap**: Kernel-smoothed resampling for density estimation
- **Block Bootstrap**: For dependent/time-series data
  - Moving block bootstrap
  - Stationary block bootstrap

### Confidence Intervals
- **Percentile Method**: Direct quantiles of bootstrap distribution
- **Basic (Pivotal) Method**: Pivot-based intervals
- **BCa (Bias-Corrected and Accelerated)**: Second-order accurate intervals
- **Bootstrap-t**: Studentized bootstrap intervals

### Jackknife Methods
- **Leave-One-Out (LOO) Jackknife**: Standard jackknife
- **Delete-d Jackknife**: Delete multiple observations
- Bias and variance estimation

### Permutation Tests
- **Two-Sample Difference Test**: Compare group means/medians
- **Correlation Test**: Test association between variables
- **Paired Test**: Compare paired observations
- Exact and Monte Carlo p-values

### Diagnostics
- Bootstrap distribution visualization
- Convergence analysis (SE vs B)
- Reproducibility via seeding

## Installation

```bash
# Clone the repository
git clone https://github.com/user/stat-bootstrap-resampling-py.git
cd stat-bootstrap-resampling-py

# Install in development mode
pip install -e ".[dev]"
```

## Quick Start

```python
import numpy as np
from resampling import bootstrap_ci, permutation_test, jackknife

# Bootstrap confidence interval for the mean
data = np.random.normal(loc=5, scale=2, size=100)
result = bootstrap_ci(data, np.mean, method='bca', B=9999)
print(f"Mean: {np.mean(data):.3f}")
print(f"95% BCa CI: [{result.ci_lower:.3f}, {result.ci_upper:.3f}]")
print(f"Bootstrap SE: {result.std_error:.3f}")

# Permutation test for two groups
group_a = np.random.normal(loc=10, scale=2, size=50)
group_b = np.random.normal(loc=12, scale=2, size=50)
perm_result = permutation_test(group_a, group_b, np.mean, B=9999)
print(f"Permutation p-value: {perm_result.p_value:.4f}")

# Jackknife bias estimation
def biased_mean(x):
    return np.mean(x) + 0.5  # Artificially biased estimator

jk_result = jackknife(data, biased_mean)
print(f"Jackknife bias estimate: {jk_result.bias:.3f}")
```

## CLI Usage

```bash
# Bootstrap CI for a mean
resampling bootstrap --data "1,2,3,4,5" --stat mean --method bca

# Permutation test
resampling permutation --group1 "1,2,3" --group2 "4,5,6"

# Run examples
resampling examples
```

## Project Structure

```
stat-bootstrap-resampling-py/
├── src/
│   └── resampling/
│       ├── __init__.py      # Package API
│       ├── bootstrap.py     # Bootstrap methods
│       ├── ci.py            # Confidence intervals
│       ├── jackknife.py     # Jackknife methods
│       ├── permutation.py   # Permutation tests
│       ├── block.py         # Block bootstrap
│       ├── cli.py           # Command-line interface
│       └── utils.py         # Utility functions
├── tests/
│   ├── test_bootstrap.py
│   ├── test_ci.py
│   ├── test_jackknife.py
│   ├── test_permutation.py
│   └── test_block.py
├── examples/
│   └── worked_examples.py
├── pyproject.toml
└── README.md
```

## Running Tests

```bash
# Install dev dependencies
pip install -e ".[dev]"

# Run all tests
pytest

# Run with coverage
pytest --cov=resampling --cov-report=term-missing

# Run specific test file
pytest tests/test_bootstrap.py -v
```

## Mathematical Background

### Bootstrap
The bootstrap (Efron, 1979) approximates the sampling distribution of a statistic by resampling with replacement from the observed data. Given data $X = (X_1, \ldots, X_n)$ and statistic $T$:

1. Draw $B$ bootstrap samples $X^{*1}, \ldots, X^{*B}$
2. Compute $T^{*b} = T(X^{*b})$ for each
3. Use empirical distribution of $T^*$ for inference

### BCa Intervals
The BCa interval (Efron & Tibshirani, 1993) applies two corrections:
- **Bias correction (z₀)**: Adjusts for median bias
- **Acceleration (â)**: Adjusts for skewness

$$[\hat{F}^{-1}(\alpha_1), \hat{F}^{-1}(\alpha_2)]$$

where $\alpha_1 = \Phi(z_0 + \frac{z_0 + z_\alpha}{1 - \hat{a}(z_0 + z_\alpha)})$

### Jackknife
The jackknife estimates bias via:
$$\hat{Bias}_{jack} = (n-1)(\bar{T}_{(\cdot)} - T_{obs})$$

### Permutation Test
Under $H_0$, labels are exchangeable. The p-value is:
$$p = \frac{\sum_{b=1}^{B+1} I(|T^{*b}| \geq |T_{obs}|)}{B + 1}$$

## References

- Efron, B. (1979). Bootstrap methods: Another look at the jackknife. *Annals of Statistics*.
- Efron, B., & Tibshirani, R. J. (1993). *An Introduction to the Bootstrap*. CRC Press.
- Davison, A. C., & Hinkley, D. V. (1997). *Bootstrap Methods and Their Application*. Cambridge University Press.
- Politis, D. N., & Romano, J. P. (1994). The stationary bootstrap. *Journal of the American Statistical Association*.

## License

MIT License
