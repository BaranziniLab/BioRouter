# Medical Survival Analysis Toolkit (R)

A comprehensive survival analysis toolkit implementing core methods from scratch, designed for medical and clinical research.

## Features

### Core Analysis Functions
- **Kaplan-Meier Estimator**: Non-parametric survival curve estimation with Greenwood's variance and confidence intervals
- **Log-Rank Test**: Mantel-Cox test for comparing survival between groups
- **Cox Proportional Hazards Regression**: Full implementation using Newton-Raphson optimization on the partial likelihood
- **Proportional Hazards Checking**: Schoenfeld residual-based diagnostics for PH assumption

### Utilities
- **Data Loading**: CSV and data.frame input with validation
- **Data Summarization**: Descriptive statistics for survival data
- **Plot Data Preparation**: Functions to prepare KM curves for ggplot2
- **Synthetic Data Generation**: Create test data with known hazard ratios

## Installation

```r
# From source
install.packages(".", repos = NULL, type = "source")

# Or load directly
source("R/data_utils.R")
source("R/kaplan_meier.R")
source("R/log_rank.R")
source("R/cox_ph.R")
source("R/ph_assumption.R")
```

## Usage

### Quick Start

```r
# Load package
library(medSurvivalAnalysis)

# Generate synthetic data with HR = 0.7
data <- generate_synthetic_survival(n_per_group = 200, hazard_ratio = 0.7)

# Kaplan-Meier estimation
km <- km_estimate(data$time, data$event, data$group)
print(km$median_survival)

# Log-rank test
lr <- log_rank_test(data$time, data$event, data$group)
print(lr$p_value)

# Cox PH regression
X <- model.matrix(~ group, data = data)[, -1]
cox <- cox_ph_model(data$time, data$event, X)
print(cox$hazard_ratios)

# Check PH assumption
ph <- check_ph_assumption(data$time, data$event, X, cox$coefficients)
print(ph$conclusion)
```

### Command-Line Interface

```bash
# Run analysis on CSV file
Rscript analysis_script.R my_data.csv --group-col treatment

# With options
Rscript analysis_script.R data.csv \
  --time-col survival_time \
  --event-col died \
  --group-col treatment_arm \
  --output results
```

### CSV Format

Your CSV should contain:
- `time`: Time to event or censoring (numeric)
- `event`: Event indicator (0 = censored, 1 = event)
- Optional: grouping variable and covariates

Example:
```csv
id,time,event,group,covariate1,covariate2
1,12.5,1,treatment,0.5,1
2,8.3,0,control,-0.2,0
3,24.1,1,treatment,1.2,1
```

## Package Structure

```
med-survival-analysis-r/
├── DESCRIPTION          # Package metadata
├── NAMESPACE            # Exported functions
├── README.md           # This file
├── analysis_script.R   # CLI entry point
├── R/
│   ├── data_utils.R    # Data loading and manipulation
│   ├── kaplan_meier.R  # KM estimation
│   ├── log_rank.R      # Log-rank test
│   ├── cox_ph.R        # Cox PH regression
│   └── ph_assumption.R # PH assumption checking
├── tests/
│   └── testthat/
│       └── test-survival-analysis.R  # Test suite
└── man/                # Documentation (generated)
```

## Implementation Details

### Kaplan-Meier Estimator
- Uses the standard product-limit formula
- Greenwood's formula for variance estimation
- Confidence intervals via normal approximation
- Handles tied event times and censoring

### Log-Rank Test
- Mantel-Haenszel chi-square statistic
- Handles multiple groups
- Provides observed vs expected event counts
- One-sided and two-sided tests

### Cox PH Regression
- Newton-Raphson optimization on partial likelihood
- Computes hazard ratios (exp(β))
- Wald test statistics and p-values
- Concordance index (C-statistic)
- Handles multiple covariates

### PH Assumption Checking
- Schoenfeld residuals
- Correlation with transformed time
- Individual and overall tests
- Interpretable conclusions

## Testing

Run the test suite:

```bash
# Using testthat
Rscript tests/testthat.R

# Or run individual test file
Rscript -e "source('tests/testthat/test-survival-analysis.R')"
```

Tests include:
- Validation of synthetic data generation
- KM estimation accuracy
- Log-rank test power and type I error
- Cox PH coefficient recovery
- PH assumption detection

## Dependencies

**Required:**
- R >= 3.5.0
- survival (for comparison tests)

**Optional:**
- testthat (for testing)
- ggplot2 (for visualization)
- MASS (for pseudo-inverse fallback)

## Mathematical Background

### Kaplan-Meier Estimator

The survival function is estimated as:

$$\hat{S}(t) = \prod_{t_i \leq t} \left(1 - \frac{d_i}{n_i}\right)$$

where $d_i$ is the number of events at time $t_i$ and $n_i$ is the number at risk.

Greenwood's variance:

$$\hat{Var}(\hat{S}(t)) = \hat{S}(t)^2 \sum_{t_i \leq t} \frac{d_i}{n_i(n_i - d_i)}$$

### Cox Proportional Hazards Model

The hazard function is:

$$h(t|X) = h_0(t) \exp(\beta^T X)$$

The partial likelihood is:

$$L(\beta) = \prod_{i: \delta_i=1} \frac{\exp(\beta^T X_i)}{\sum_{j \in R(t_i)} \exp(\beta^T X_j)}$$

Newton-Raphson iterates: $\beta^{(k+1)} = \beta^{(k)} - H^{-1} U$

where $U$ is the score vector and $H$ is the Hessian matrix.

### Schoenfeld Residuals

For PH diagnostics, scaled Schoenfeld residuals are computed:

$$r_{S,i} = X_i - \bar{X}_w$$

where $\bar{X}_w$ is the risk-set weighted average of covariates.

Correlation with $\log(t)$ indicates PH violation.

## License

MIT License

## Author

BioRouter Team (Baranzini Lab, UCSF)
