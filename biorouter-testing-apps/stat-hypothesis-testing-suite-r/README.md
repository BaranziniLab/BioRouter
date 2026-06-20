# Statistical Hypothesis Testing Suite for R

A comprehensive hypothesis testing package implemented from scratch in base R, providing parametric, non-parametric, categorical, and normality tests with tidy output.

## Features

### Parametric Tests
- **One-sample t-test**: Compare a sample mean to a hypothesized value
- **Two-sample t-test**: Compare means of two independent groups (equal variance)
- **Paired t-test**: Compare means of paired/dependent samples
- **Welch's t-test**: Two-sample t-test without equal variance assumption
- **One-way ANOVA**: Compare means across multiple groups
- **Two-way ANOVA**: Two-factor analysis of variance
- **F-test**: Compare two variances
- **Pearson correlation**: Test linear association between variables
- **Simple linear regression**: Single predictor regression with coefficient tests
- **Multiple regression**: Multiple predictor regression with coefficient tests

### Non-Parametric Tests
- **Wilcoxon rank-sum**: Two-sample comparison without normality assumption
- **Wilcoxon signed-rank**: Paired comparison without normality assumption
- **Kruskal-Wallis**: Non-parametric one-way ANOVA
- **Mann-Whitney U**: Alternative formulation of rank-sum test
- **Spearman correlation**: Rank-based correlation
- **Sign test**: Non-parametric paired comparison

### Categorical Tests
- **Chi-square goodness-of-fit**: Test observed vs expected frequencies
- **Chi-square independence**: Test association in contingency tables
- **Fisher's exact**: Exact test for 2x2 tables (small samples)
- **McNemar's test**: Paired nominal data (before/after)

### Normality Tests
- **Shapiro-Wilk**: Test for normality
- **Kolmogorov-Smirnov**: Test against normal distribution

### Corrections & Power
- **Bonferroni**: Conservative family-wise error correction
- **Holm**: Step-down correction (less conservative)
- **BH-FDR**: Benjamini-Hochberg false discovery rate
- **Power analysis**: Calculate power for t-tests and ANOVA
- **Sample size**: Determine required sample size for desired power

### Reporting
- **hyp_report()**: Unified reporting function with assumption checks

## Installation

```r
# Install from source
install.packages(NULL, repos = NULL, type = "source", path = ".")

# Or load all files
lapply(list.files("R", full.names = TRUE), source)
```

## Usage

```r
library(hypTestSuite)

# One-sample t-test
x <- rnorm(30, mean = 5.2, sd = 1)
hyp_one_sample_t(x, mu = 5.0)

# Two-sample t-test
x <- rnorm(20, mean = 5)
y <- rnorm(20, mean = 6)
hyp_two_sample_t(x, y)

# One-way ANOVA
df <- data.frame(y = rnorm(30), g = factor(rep(1:3, each=10)))
hyp_one_way_anova(y ~ g, data = df)

# Chi-square test
tbl <- matrix(c(10, 20, 30, 40), nrow = 2)
hyp_chi_square_independence(tbl)

# Multiple comparison correction
p_vals <- c(0.01, 0.04, 0.03, 0.005, 0.10)
corr_bonferroni(p_vals)
corr_holm(p_vals)
corr_bh_fdr(p_vals)

# Power analysis
power_t_test(n = 30, d = 0.5)
sample_size_t_test(power = 0.80, d = 0.5)

# Full report with assumption checks
x <- c(85, 90, 78, 92, 88)
y <- c(80, 85, 75, 88, 82)
hyp_report(x, y, test = "paired_t", alpha = 0.05)
```

## Output Format

All tests return a `hyp_result` object with:
- `test_name`: Name of the test
- `statistic`: Test statistic value
- `df`: Degrees of freedom
- `p_value`: Computed p-value
- `effect_size`: Effect size estimate (Cohen's d, eta-squared, etc.)
- `ci_lower`, `ci_upper`: Confidence interval bounds
- `alternative`: Hypothesis direction
- `method`: Description of the method
- `extra`: Additional details

## Testing

```r
# Run all tests
library(testthat)
test_dir("tests/testthat")

# Run specific test file
test_file("tests/testthat/test-parametric.R")
```

## Implementation Notes

- All tests are implemented from scratch using base R
- Statistical distributions (t, F, chi-squared, normal) computed using special functions
- Validated against base R's built-in functions within tolerance
- Effect sizes computed using standard formulas
- Confidence intervals use appropriate methods (t-based, Fisher z, Wilson score)

## License

MIT
