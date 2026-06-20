#!/usr/bin/env Rscript
# ---------------------------------------------------------------------------
# driver.R — Demonstrate myglm library on all three families
# ---------------------------------------------------------------------------

cat("=== myglm: GLM from Scratch in R ===\n\n")

# Source all library files
args = commandArgs(trailingOnly = FALSE)
script_arg = grep("^--file=", args, value = TRUE)
if (length(script_arg) > 0) {
  script_path = normalizePath(sub("^--file=", "", script_arg[1]))
  lib_dir = file.path(dirname(script_path), "..", "..", "R")
} else {
  lib_dir = file.path(getwd(), "R")
}
if (!dir.exists(lib_dir)) lib_dir = file.path(getwd(), "R")
for (f in list.files(lib_dir, pattern = "\\.R$", full.names = TRUE)) source(f)

# --- Gaussian ---------------------------------------------------------------
cat("--- Gaussian (identity link) ---\n")
set.seed(42)
n = 300
x1 = rnorm(n)
x2 = rnorm(n)
y_gauss = 2 - 1.5 * x1 + 0.8 * x2 + rnorm(n, sd = 0.5)
dat_g = data.frame(y = y_gauss, x1 = x1, x2 = x2)

fit_g = my_glm(y ~ x1 + x2, data = dat_g, family = "gaussian")
ref_g = glm(y ~ x1 + x2, data = dat_g, family = stats::gaussian())

cat("Coefficients (my_glm):", round(fit_g$coefficients, 4), "\n")
cat("Coefficients (glm):   ", round(coef(ref_g), 4), "\n")
cat("Max abs diff:         ", round(max(abs(fit_g$coefficients - coef(ref_g))), 10), "\n")
cat("Deviance match:       ", abs(fit_g$deviance - deviance(ref_g)) < 1e-6, "\n")
cat("AIC match:            ", abs(fit_g$aic - AIC(ref_g)) < 1e-4, "\n\n")

# --- Binomial (logit) -------------------------------------------------------
cat("--- Binomial (logit link) ---\n")
set.seed(123)
x1b = rnorm(n)
x2b = rnorm(n)
eta_b = -0.5 + 1.2 * x1b - 0.7 * x2b
prob_b = 1 / (1 + exp(-eta_b))
y_bin = rbinom(n, 1, prob_b)
dat_b = data.frame(y = y_bin, x1 = x1b, x2 = x2b)

fit_b = my_glm(y ~ x1 + x2, data = dat_b, family = "binomial")
ref_b = glm(y ~ x1 + x2, data = dat_b, family = stats::binomial())

cat("Coefficients (my_glm):", round(fit_b$coefficients, 4), "\n")
cat("Coefficients (glm):   ", round(coef(ref_b), 4), "\n")
cat("Max abs diff:         ", round(max(abs(fit_b$coefficients - coef(ref_b))), 10), "\n\n")

# --- Poisson (log link) -----------------------------------------------------
cat("--- Poisson (log link) ---\n")
set.seed(456)
x1p = rnorm(n)
x2p = rnorm(n)
eta_p = 0.5 + 0.3 * x1p - 0.2 * x2p
mu_p = exp(eta_p)
y_pois = rpois(n, mu_p)
dat_p = data.frame(y = y_pois, x1 = x1p, x2 = x2p)

fit_p = my_glm(y ~ x1 + x2, data = dat_p, family = "poisson")
ref_p = glm(y ~ x1 + x2, data = dat_p, family = stats::poisson())

cat("Coefficients (my_glm):", round(fit_p$coefficients, 4), "\n")
cat("Coefficients (glm):   ", round(coef(ref_p), 4), "\n")
cat("Max abs diff:         ", round(max(abs(fit_p$coefficients - coef(ref_p))), 10), "\n\n")

# --- Summary + Predictions --------------------------------------------------
cat("--- Summary ---\n")
print(summary(fit_g))

cat("--- Predictions (first 5 rows) ---\n")
pred = predict(fit_g, type = "response", ci = TRUE)
cat("  Fit   Lower   Upper\n")
for (i in 1:5) {
  cat(sprintf("  %.3f  %.3f  %.3f\n", pred$fit[i], pred$lwr[i], pred$upr[i]))
}

cat("\n=== All done ===\n")
