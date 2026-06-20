# myglm: Generalized Linear Models from Scratch in R

A from-scratch implementation of GLM fitting via **Iteratively Reweighted Least Squares (IRLS)** in base R — no reliance on `glm()` for the core fitting algorithm.

## Features

- **Families**: gaussian (identity link), binomial (logit/probit/cloglog), poisson (log link)
- **IRLS engine**: Iteratively reweighted least squares with QR decomposition
- **Design matrices**: Formula-based model matrix with factor/dummy coding and intercept
- **Inference**: Coefficient estimates, standard errors (Fisher information), z-statistics, p-values
- **Goodness of fit**: Deviance, null deviance, AIC, residual degrees of freedom
- **Predictions**: Link-scale and response-scale with delta-method confidence intervals
- **Diagnostics**: Deviance residuals, Pearson residuals, working residuals, response residuals, leverage (hat values)
- **S3 methods**: `print`, `summary`, `predict`

## Project Structure

```
stat-glm-from-scratch-r/
├── DESCRIPTION
├── NAMESPACE
├── R/
│   ├── family.R        # Family objects (gaussian, binomial, poisson) + link functions
│   ├── formula.R       # Design matrix construction from formula + data
│   ├── irls.R          # IRLS fitting engine
│   ├── glm_fit.R       # Top-level my_glm() interface
│   ├── predict.R       # Prediction with CIs on link/response scale
│   ├── diagnostics.R   # Residuals (deviance, Pearson, working) + leverage
│   ├── summary.R       # print/summary S3 methods
│   └── sim-data.R      # Synthetic data generators with known coefficients
├── tests/
│   ├── testthat.R
│   └── testthat/
│       ├── test-family.R       # Link round-trips, variance functions
│       ├── test-glm_fit.R      # IRLS recovers true coefficients, matches glm()
│       ├── test-predict.R      # Prediction accuracy and CI coverage
│       └── test-diagnostics.R  # Residual properties, leverage bounds
└── inst/
    └── scripts/
        └── driver.R   # Rscript driver demonstrating all families
```

## Usage

### As an R package

```r
# Install from source
devtools::load_all(".")

# Fit a Gaussian GLM
fit = my_glm(y ~ x1 + x2, data = my_data, family = gaussian())
summary(fit)

# Fit a logistic regression
fit_bin = my_glm(y ~ x1 + x2, data = my_data, family = binomial())
predict(fit_bin, newdata = new_data, type = "response", ci = TRUE)

# Fit a Poisson model
fit_poi = my_glm(count ~ x1 + x2, data = my_data, family = poisson())
```

### As a script

```bash
Rscript inst/scripts/driver.R
```

## Validation

All coefficient estimates are validated against R's built-in `glm()` within machine precision tolerances. Tests use synthetic data with known true coefficients and verify:

- Coefficient recovery (true values within sampling tolerance)
- Exact match with `glm()` coefficients (tolerance < 1e-5)
- Standard error agreement with `glm()`
- Deviance and AIC agreement
- Prediction accuracy on both link and response scales
- Leverage properties (0 ≤ h_ii < 1, sum = rank)

## Algorithm

IRLS iterates:
1. Compute working responses: z = η + (y - μ) / g'(μ)
2. Compute working weights: W = diag(w · [g'(μ)]² / V(μ))
3. Solve weighted least squares: β = (X'WX)^{-1} X'Wz
4. Update η = Xβ, μ = g^{-1}(η)
5. Check deviance convergence

## License

MIT
