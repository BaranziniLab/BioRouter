# biomarkerDiscovR

A comprehensive R toolkit for **biomarker discovery and feature selection** in high-dimensional biomedical data.

## Overview

`biomarkerDiscovR` provides an end-to-end pipeline for identifying predictive biomarkers from omics, clinical, or other high-dimensional datasets. The toolkit is implemented in base R with no external dependencies beyond standard CRAN packages.

## Features

### Preprocessing
- **Low-variance filtering** — remove features with variance below a threshold
- **Missing-value handling** — filter high-missing features, impute remaining (median/mean/zero)
- **Normalization** — z-score, robust z-score, or min-max scaling

### Univariate Screening
- **t-test** / **Wilcoxon rank-sum** for binary outcomes
- **Pearson correlation** for continuous outcomes
- **Multiple-testing correction**: Bonferroni and Benjamini-Hochberg (BH/FDR)

### Multivariate Feature Selection
- **LASSO / Elastic-Net** — coordinate-descent logistic regression (no glmnet dependency)
- **Recursive Feature Elimination (RFE)** — iteratively remove least important features
- **Stability Selection** — repeated subsampling to identify consistently selected features

### Model Evaluation
- **K-fold cross-validation** with AUC and accuracy metrics
- **Panel ranking** — evaluate and compare multiple candidate biomarker panels
- **Effect-size reporting** — per-feature statistics, p-values, and selection frequencies

### Reporting
- Formatted text report with panel rankings, effect sizes, and selected features
- CSV exports for downstream analysis

## Project Structure

```
med-biomarker-discovery-r/
├── DESCRIPTION           # R package metadata
├── NAMESPACE             # Exported functions
├── LICENSE               # MIT license
├── README.md             # This file
├── Rscript.R             # Runnable CLI script
├── R/                    # Source modules
│   ├── utils.R           # Utility functions (AUC, CV folds, etc.)
│   ├── preprocess.R      # Preprocessing pipeline
│   ├── univariate.R      # Univariate screening
│   ├── lasso.R           # LASSO / elastic-net (coordinate descent)
│   ├── rfe.R             # Recursive feature elimination
│   ├── stability.R       # Stability selection
│   ├── evaluation.R      # Cross-validation evaluation
│   ├── ranker.R          # Panel ranking
│   ├── report.R          # Reporting / summaries
│   ├── synthetic.R       # Synthetic data generation
│   └── pipeline.R        # Main pipeline tying all modules together
├── tests/
│   ├── run_tests.R       # Test harness (no testthat dependency)
│   └── testthat/
│       ├── test-utils.R
│       ├── test-preprocess.R
│       ├── test-univariate.R
│       ├── test-lasso.R
│       ├── test-rfe.R
│       ├── test-stability.R
│       ├── test-evaluation.R
│       ├── test-ranker.R
│       ├── test-synthetic.R
│       └── test-pipeline.R  (integration)
└── inst/extdata/         # (reserved for example data)
```

## Quick Start

### Running with synthetic data (demo)

```bash
Rscript Rscript.R --demo --output ./output
```

### Running with your data

```bash
Rscript Rscript.R --data my_data.csv --outcome outcome --output ./output
```

Your CSV should have samples in rows, features in columns, and an outcome column.

### Running the test suite

```bash
Rscript tests/run_tests.R
```

## Usage in R

```r
# Source all modules
for (f in list.files("R", pattern = "\\.R$", full.names = TRUE)) source(f)

# Generate synthetic data
data <- create_synthetic_data(n_samples = 200, n_features = 500,
                               n_informative = 15, effect_size = 1.5)

# Run the full pipeline
result <- pipeline(data$X, data$y, verbose = TRUE)

# Examine the ranked panels
print(result$ranking$ranking)

# View the report
cat(result$report)
```

## Methods

### LASSO Coordinate Descent

The LASSO implementation uses cyclic coordinate descent for logistic regression with L1 (and optional L2) penalties. Each coordinate update uses the soft-thresholding operator:

```
β_j ← S(∇_j L / n, λα) / (∑x_ij²/n + λ(1-α))
```

### Stability Selection

Repeatedly subsamples the data (default 100 iterations, 70% subsamples), fits LASSO on each, and ranks features by selection frequency. Features selected in ≥ threshold fraction of iterations are retained.

### Cross-Validation

Standard k-fold CV (default 5 folds) with per-fold AUC and accuracy computation. Panels are ranked by mean CV AUC.

## License

MIT License. See [LICENSE](LICENSE) for details.
