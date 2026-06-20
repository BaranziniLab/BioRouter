#!/usr/bin/env Rscript
#' run_analysis.R — Rscript driver for statSurvivalPower
#'
#' Demonstrates the full toolkit: effect sizes, power calculations,
#' sample size determination, survival analysis, solver, and power curves.

# ---- Setup ----
library(statSurvivalPower)

cat("\n========================================\n")
cat("  statSurvivalPower — Demo Analysis\n")
cat("========================================\n\n")

# ---- 1. Effect-size conversions ----
cat("--- Effect-Size Conversions ---\n")
d <- 0.5
f_val <- cohen_d_to_f(d)
cat(sprintf("  Cohen's d = %.2f  =>  Cohen's f = %.4f\n", d, f_val))

h_val <- cohen_d_to_h(d)
cat(sprintf("  Cohen's d = %.2f  =>  Cohen's h = %.4f\n", d, h_val))

w_val <- cohen_h_to_w(h_val)
cat(sprintf("  Cohen's h = %.4f =>  Cohen's w = %.4f\n", h_val, w_val))

es <- effect_size_from_cohens_d(d)
cat(sprintf("  eta-squared = %.4f, omega-squared = %.4f\n", es$eta_sq, es$omega_sq))
cat("\n")

# ---- 2. Two-sample t-test ----
cat("--- Two-Sample t-Test ---\n")
result_t <- sample_size_t_test(d = 0.5, power = 0.80, type = "two.sample")
cat(sprintf("  Required n per group: %d (achieved power: %.4f)\n",
            result_t$n, result_t$achieved_power))

pw_t <- power_t_test(n = 30, d = 0.5, type = "two.sample")
cat(sprintf("  Power at n=30, d=0.50:  %.4f\n", pw_t))
cat("\n")

# ---- 3. One-way ANOVA ----
cat("--- One-Way ANOVA ---\n")
result_a <- sample_size_anova(k = 3, f = 0.25, power = 0.80)
cat(sprintf("  Required n per group: %d (achieved power: %.4f)\n",
            result_a$n, result_a$achieved_power))
cat("\n")

# ---- 4. Two-proportion test ----
cat("--- Two-Proportion Test ---\n")
result_p <- sample_size_two_proportion(p1 = 0.30, p2 = 0.50, power = 0.80)
cat(sprintf("  Required n per group: %d (achieved power: %.4f)\n",
            result_p$n, result_p$achieved_power))
cat("\n")

# ---- 5. Correlation test ----
cat("--- Correlation Test ---\n")
result_c <- sample_size_correlation(r = 0.3, power = 0.80)
cat(sprintf("  Required n: %d (achieved power: %.4f)\n",
            result_c$n, result_c$achieved_power))
cat("\n")

# ---- 6. Chi-square test ----
cat("--- Chi-Square Test ---\n")
result_chi <- sample_size_chi_square(w = 0.3, df = 1, power = 0.80)
cat(sprintf("  Required n: %d (achieved power: %.4f)\n",
            result_chi$n, result_chi$achieved_power))
cat("\n")

# ---- 7. Survival / log-rank (Schoenfeld) ----
cat("--- Survival / Log-Rank (Schoenfeld) ---\n")
result_surv <- power_survival_logrank(hr = 0.7, power = 0.80, alpha = 0.05)
cat(sprintf("  Schoenfeld events needed: %d\n", result_surv$n_events_schoenfeld))
cat(sprintf("  Freedman events needed:   %d\n", result_surv$n_events_freedman))
cat("\n")

# ---- 8. Universal solver ----
cat("--- Universal Solver ---\n")
sol <- solve_power(power_t_test, target = "d", target_value = 0.80,
                   n = 30, type = "two.sample", hi = 3.0)
cat(sprintf("  To achieve 80%% power with n=30 (two-sample): d = %.4f\n",
            sol$found_value))
cat("\n")

# ---- 9. Power curves + ASCII plot ----
cat("--- Power Curve (t-test, two-sample, d=0.5) ---\n")
curves <- power_curves(power_t_test, varying = "n", d = 0.5,
                       n_range = c(5, 150), type = "two.sample")
print_ascii_plot(curves$x, curves$power, xlab = "n per group",
                 ylab = "Power", title = "Power vs Sample Size")

# ---- 10. Full report ----
cat("--- Power Report ---\n")
print_power_report(power_t_test, n = 50, d = 0.5, type = "two.sample",
                   test_name = "Two-Sample t-Test")

cat("\n========================================\n")
cat("  Analysis complete.\n")
cat("========================================\n")
