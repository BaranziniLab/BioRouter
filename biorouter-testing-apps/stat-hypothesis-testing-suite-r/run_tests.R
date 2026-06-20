#!/usr/bin/env Rscript
# run_tests.R - Driver script for the hypothesis testing suite
# Runs all tests and demonstrates the reporting function

cat("=== Statistical Hypothesis Testing Suite - R Package ===\n")
cat("Running tests and demonstrations...\n\n")

# Source the package files
r_files <- list.files("R", pattern = "\\.R$", full.names = TRUE)
for (f in r_files) {
  source(f)
}

# ---- Test 1: One-sample t-test ----
cat("--- Test 1: One-Sample t-test ---\n")
set.seed(42)
x <- rnorm(30, mean = 5.2, sd = 1)
result <- hyp_one_sample_t(x, mu = 5.0)
base <- t.test(x, mu = 5.0)
cat(sprintf("Our p-value: %.6f\n", result$p_value))
cat(sprintf("Base R p-value: %.6f\n", base$p.value))
cat(sprintf("Difference: %.2e\n\n", abs(result$p_value - base$p.value)))

# ---- Test 2: Two-sample t-test ----
cat("--- Test 2: Two-Sample t-test ---\n")
x <- c(5.2, 4.8, 5.5, 5.1, 4.9, 5.3, 5.0, 4.7, 5.4, 5.2)
y <- c(3.1, 3.5, 2.9, 3.3, 3.2, 3.4, 3.0, 3.6, 3.1, 3.3)
result <- hyp_two_sample_t(x, y)
base <- t.test(x, y, var.equal = TRUE)
cat(sprintf("Our p-value: %.6f\n", result$p_value))
cat(sprintf("Base R p-value: %.6f\n", base$p.value))
cat(sprintf("Difference: %.2e\n\n", abs(result$p_value - base$p.value)))

# ---- Test 3: One-way ANOVA ----
cat("--- Test 3: One-Way ANOVA ---\n")
df <- data.frame(
  value = c(rnorm(10, mean = 5), rnorm(10, mean = 6), rnorm(10, mean = 7)),
  group = factor(rep(c("A", "B", "C"), each = 10))
)
result <- hyp_one_way_anova(value ~ group, data = df)
base <- aov(value ~ group, data = df)
base_summary <- summary(base)
cat(sprintf("Our F-stat: %.6f, Base F-stat: %.6f\n",
            result$statistic, base_summary[[1]]$`F value`[1]))
cat(sprintf("Our p-value: %.6f, Base p-value: %.6f\n\n",
            result$p_value, base_summary[[1]]$`Pr(>F)`[1]))

# ---- Test 4: Chi-square ----
cat("--- Test 4: Chi-Square Independence ---\n")
tbl <- matrix(c(10, 20, 30, 40), nrow = 2)
result <- hyp_chi_square_independence(tbl)
base <- chisq.test(tbl)
cat(sprintf("Our p-value: %.6f\n", result$p_value))
cat(sprintf("Base R p-value: %.6f\n", base$p.value))
cat(sprintf("Difference: %.2e\n\n", abs(result$p_value - base$p.value)))

# ---- Test 5: Multiple Comparison Corrections ----
cat("--- Test 5: Corrections ---\n")
p_vals <- c(0.01, 0.04, 0.03, 0.005, 0.10)
cat("Raw p-values:", p_vals, "\n")
bonf <- corr_bonferroni(p_vals)
holm <- corr_holm(p_vals)
bh <- corr_bh_fdr(p_vals)
cat("Bonferroni adjusted:", round(bonf$p_adjusted, 4), "\n")
cat("Holm adjusted:", round(holm$p_adjusted, 4), "\n")
cat("BH-FDR adjusted:", round(bh$p_adjusted, 4), "\n\n")

# ---- Test 6: Power Analysis ----
cat("--- Test 6: Power Analysis ---\n")
pw <- power_t_test(n = 30, d = 0.5)
cat(sprintf("Power for n=30, d=0.5: %.4f\n", pw$power))
ss <- sample_size_t_test(power = 0.80, d = 0.5)
cat(sprintf("Sample size for 80%% power, d=0.5: n=%d\n\n", ss$n))

# ---- Test 7: Full Report ----
cat("--- Test 7: Full Report ---\n")
x <- c(85, 90, 78, 92, 88, 76, 95, 89, 84, 91)
y <- c(80, 85, 75, 88, 82, 72, 90, 85, 80, 87)
hyp_report(x, y, test = "paired_t", alpha = 0.05)

cat("\n=== All demonstrations completed ===\n")
