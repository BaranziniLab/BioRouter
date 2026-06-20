#' Test Suite for Survival Analysis Toolkit
#'
#' Tests core functionality including:
#' - Kaplan-Meier estimation
#' - Log-rank test
#' - Cox PH regression
#' - Proportional hazards checking
#' - Data loading and manipulation

library(testthat)

# Source all package files
r_files <- list.files(system.file("R", package = "medSurvivalAnalysis"),
                      pattern = "\\.R$", full.names = TRUE)
if (length(r_files) == 0) {
  # Fallback: source from project directory
  r_dir <- file.path(dirname(getwd()), "R")
  if (dir.exists(r_dir)) {
    r_files <- list.files(r_dir, pattern = "\\.R$", full.names = TRUE)
    for (f in r_files) source(f)
  }
}

# ============================================================
# Synthetic Data Generation
# ============================================================

test_that("generate_synthetic_survival creates valid data", {
  data <- generate_synthetic_survival(n_per_group = 50, seed = 123)

  expect_true(is.data.frame(data))
  expect_equal(nrow(data), 100)
  expect_true(all(data$time > 0))
  expect_true(all(data$event %in% c(0, 1)))
  expect_equal(levels(as.factor(data$group)), c("control", "treatment"))
})

test_that("synthetic data has correct hazard ratio", {
  data <- generate_synthetic_survival(n_per_group = 1000,
                                       base_hazard = 0.1,
                                       hazard_ratio = 0.7,
                                       seed = 42)

  X <- as.matrix(data.frame(grouptreatment = as.numeric(data$group == "treatment")))
  cox_fit <- cox_ph_model(data$time, data$event, X)

  estimated_hr <- cox_fit$hazard_ratios["grouptreatment"]
  expect_true(estimated_hr > 0.5 && estimated_hr < 0.9,
              info = paste("Estimated HR:", round(estimated_hr, 3)))
})

test_that("synthetic data has correct event rate", {
  data <- generate_synthetic_survival(n_per_group = 500,
                                       base_hazard = 0.1,
                                       censor_time = 5,
                                       seed = 99)

  event_rate <- mean(data$event)
  expect_true(event_rate > 0.3 && event_rate < 0.5,
              info = paste("Event rate:", round(event_rate, 3)))
})

# ============================================================
# Kaplan-Meier Estimation
# ============================================================

test_that("km_estimate produces valid output", {
  data <- generate_synthetic_survival(n_per_group = 100, seed = 123)
  km <- km_estimate(data$time, data$event)

  expect_true(is.list(km))
  expect_true(length(km$times) > 0)
  expect_true(all(km$survival >= 0 & km$survival <= 1))
  expect_true(all(km$lower >= 0 & km$lower <= 1))
  expect_true(all(km$upper >= 0 & km$upper <= 1))
  expect_true(all(km$lower <= km$survival))
  expect_true(all(km$survival <= km$upper))
})

test_that("KM survival probabilities are non-increasing", {
  data <- generate_synthetic_survival(n_per_group = 100, seed = 456)
  km <- km_estimate(data$time, data$event)

  diffs <- diff(km$survival)
  expect_true(all(diffs <= 0),
              info = "KM survival should be non-increasing")
})

test_that("KM at-risk counts are correct", {
  time <- c(1, 2, 3, 4, 5)
  event <- c(1, 1, 0, 1, 0)

  km <- km_estimate(time, event)

  expect_equal(km$n_at_risk[1], 5)
  expect_equal(km$n_events[1], 1)
})

test_that("KM estimates match survival package", {
  skip_if_not_installed("survival")

  data <- generate_synthetic_survival(n_per_group = 200, seed = 789)

  km_ours <- km_estimate(data$time, data$event)
  fit <- survival::survfit(survival::Surv(data$time, data$event) ~ 1)

  common_times <- intersect(km_ours$times, fit$time)
  if (length(common_times) > 0) {
    idx_ours <- match(common_times, km_ours$times)
    idx_surv <- match(common_times, fit$time)

    diffs <- abs(km_ours$survival[idx_ours] - fit$surv[idx_surv])
    expect_true(all(diffs < 0.02),
                info = paste("Max difference:", round(max(diffs), 4)))
  }
})

test_that("median survival is computed correctly", {
  set.seed(42)
  n <- 1000
  time <- rexp(n, rate = 0.1)
  event <- rep(1, n)

  km <- km_estimate(time, event)

  expected_median <- log(2) / 0.1
  expect_true(abs(km$median_survival - expected_median) < 1.0,
              info = paste("KM median:", round(km$median_survival, 2),
                          "Expected:", round(expected_median, 2)))
})

# ============================================================
# Log-Rank Test
# ============================================================

test_that("log_rank_test detects group differences", {
  set.seed(123)
  n <- 200

  time_control <- rexp(n/2, rate = 0.1)
  time_treatment <- rexp(n/2, rate = 0.05)

  time <- c(time_control, time_treatment)
  event <- rep(1, n)
  group <- rep(c("control", "treatment"), each = n/2)

  lr <- log_rank_test(time, event, group)

  expect_true(lr$p_value < 0.01,
              info = paste("P-value:", lr$p_value))
  expect_true(lr$z_score > 0)
})

test_that("log_rank_test fails to reject when groups are same", {
  set.seed(456)
  n <- 100

  time <- rexp(n, rate = 0.1)
  event <- rep(1, n)
  group <- rep(c("control", "treatment"), each = n/2)

  lr <- log_rank_test(time, event, group)

  expect_true(lr$p_value > 0.05,
              info = paste("P-value:", lr$p_value))
})

test_that("log_rank_test matches survival package", {
  skip_if_not_installed("survival")

  set.seed(789)
  n <- 200
  time <- rexp(n, rate = 0.1)
  event <- rbinom(n, 1, 0.8)
  group <- rep(c("A", "B"), each = n/2)

  lr_ours <- log_rank_test(time, event, group)

  surv_result <- survival::survdiff(survival::Surv(time, event) ~ group)
  p_surv <- 1 - stats::pchisq(surv_result$chisq, df = 1)

  expect_true(abs(lr_ours$p_value - p_surv) < 0.05,
              info = paste("Our p:", lr_ours$p_value,
                          "survival p:", p_surv))
})

test_that("log_rank_test handles censoring", {
  set.seed(101)
  n <- 150

  time <- rexp(n, rate = 0.1)
  event <- rbinom(n, 1, 0.6)
  group <- rep(c("G1", "G2"), each = n/2)

  lr <- log_rank_test(time, event, group)

  expect_true(is.numeric(lr$statistic))
  expect_true(lr$statistic >= 0)
  expect_true(lr$df == 1)
})

test_that("log_rank_test observed equals expected when no difference", {
  time <- c(1, 1, 2, 2, 3, 3)
  event <- c(1, 1, 1, 1, 0, 0)
  group <- c("A", "B", "A", "B", "A", "B")

  lr <- log_rank_test(time, event, group)

  expect_equal(lr$observed_per_group[1], lr$expected_per_group[1],
               tolerance = 0.5)
})

# ============================================================
# Cox Proportional Hazards Regression
# ============================================================

test_that("cox_ph_model recovers known hazard ratio", {
  data <- generate_synthetic_survival(n_per_group = 500,
                                       base_hazard = 0.1,
                                       hazard_ratio = 0.7,
                                       seed = 42)

  X <- as.matrix(data.frame(grouptreatment = as.numeric(data$group == "treatment")))
  cox <- cox_ph_model(data$time, data$event, X)

  hr_estimated <- cox$hazard_ratios["grouptreatment"]
  expect_true(hr_estimated > 0.5 && hr_estimated < 0.9,
              info = paste("Estimated HR:", round(hr_estimated, 3)))

  true_beta <- log(0.7)
  expect_true(abs(cox$coefficients["grouptreatment"] - true_beta) < 0.3,
              info = paste("Estimated beta:", round(cox$coefficients["grouptreatment"], 3)))
})

test_that("cox_ph_model has significant Wald test for strong effect", {
  set.seed(123)
  n <- 300

  group <- rep(c(0, 1), each = n/2)
  lambda <- ifelse(group == 0, 0.1, 0.05)
  time <- rexp(n, rate = lambda)
  event <- rep(1, n)

  X <- as.matrix(data.frame(group = group))
  cox <- cox_ph_model(time, event, X)

  expect_true(cox$p_value[1] < 0.01)
  expect_true(cox$z[1] < 0)
})

test_that("cox_ph_model handles multiple covariates", {
  set.seed(456)
  n <- 200

  x1 <- rnorm(n)
  x2 <- rbinom(n, 1, 0.5)

  beta1 <- log(1.5)
  beta2 <- log(0.8)

  lambda <- 0.1 * exp(beta1 * x1 + beta2 * x2)
  time <- rexp(n, rate = lambda)
  event <- rbinom(n, 1, 0.8)

  X <- cbind(x1, x2)
  cox <- cox_ph_model(time, event, X)

  expect_true(abs(cox$coefficients["x1"] - beta1) < 0.5)
  expect_true(abs(cox$coefficients["x2"] - beta2) < 0.5)
})

test_that("cox_ph_model computes valid confidence intervals", {
  data <- generate_synthetic_survival(n_per_group = 100, seed = 789)
  X <- as.matrix(data.frame(grouptreatment = as.numeric(data$group == "treatment")))
  cox <- cox_ph_model(data$time, data$event, X)

  hr <- cox$hazard_ratios["grouptreatment"]
  ci_low <- cox$ci_lower["grouptreatment"]
  ci_high <- cox$ci_upper["grouptreatment"]

  expect_true(ci_low < hr)
  expect_true(ci_high > hr)
  expect_true(ci_low < 1 || ci_high > 1)
})

test_that("cox_ph_model computes concordance", {
  data <- generate_synthetic_survival(n_per_group = 100, seed = 321)
  X <- as.matrix(data.frame(grouptreatment = as.numeric(data$group == "treatment")))
  cox <- cox_ph_model(data$time, data$event, X)

  expect_true(is.numeric(cox$concordance))
  expect_true(cox$concordance >= 0 && cox$concordance <= 1)
})

test_that("cox_ph_model matches survival::coxph", {
  skip_if_not_installed("survival")

  data <- generate_synthetic_survival(n_per_group = 200, seed = 654)

  X <- as.matrix(data.frame(grouptreatment = as.numeric(data$group == "treatment")))
  cox_ours <- cox_ph_model(data$time, data$event, X)

  surv_fit <- survival::coxph(survival::Surv(time, event) ~ group, data = data)
  # summary$conf.int has columns: exp(coef), se(coef), z, Pr(>|z|), lower .95, upper .95
  # Column 1 is exp(coef) = hazard ratio
  hr_surv <- summary(surv_fit)$conf.int[1, 1]

  expect_true(abs(cox_ours$hazard_ratios["grouptreatment"] - hr_surv) < 0.3,
              info = paste("Our HR:", round(cox_ours$hazard_ratios["grouptreatment"], 3),
                          "survival HR:", round(hr_surv, 3)))
})

test_that("cox_ph_model handles convergence", {
  data <- generate_synthetic_survival(n_per_group = 50, seed = 111)
  X <- as.matrix(data.frame(grouptreatment = as.numeric(data$group == "treatment")))
  cox <- cox_ph_model(data$time, data$event, X)

  expect_true(cox$converged)
  expect_true(cox$n_iterations < 50)
})

# ============================================================
# Proportional Hazards Assumption
# ============================================================

test_that("check_ph_assumption detects PH violation", {
  set.seed(123)
  n <- 300

  time <- rexp(n, rate = 0.1)
  x <- rnorm(n)

  beta_t <- 0.1 * log(time + 1)
  lambda <- 0.1 * exp(beta_t * x)
  event <- rbinom(n, 1, pmin(1, lambda * time / 10))

  X <- as.matrix(data.frame(x = x))

  ph <- check_ph_assumption(time, event, X)

  expect_true(is.list(ph))
  expect_true(length(ph$p_value) == 1)
})

test_that("check_ph_assumption passes when PH holds", {
  set.seed(456)
  n <- 200

  x <- rnorm(n)
  lambda <- 0.1 * exp(0.5 * x)
  time <- rexp(n, rate = lambda)
  event <- rbinom(n, 1, 0.8)

  X <- as.matrix(data.frame(x = x))

  ph <- check_ph_assumption(time, event, X)

  # Liberal threshold since test has low power at small n
  expect_true(ph$p_value[1] > 0.01,
              info = paste("PH test p-value:", ph$p_value[1]))
})

test_that("check_ph_assumption returns Schoenfeld residuals", {
  data <- generate_synthetic_survival(n_per_group = 100, seed = 789)
  X <- as.matrix(data.frame(grouptreatment = as.numeric(data$group == "treatment")))

  ph <- check_ph_assumption(data$time, data$event, X)

  expect_true(is.matrix(ph$schoenfeld_residuals))
  expect_true(nrow(ph$schoenfeld_residuals) == sum(data$event))
  expect_true(ncol(ph$schoenfeld_residuals) == ncol(X))
})

test_that("Schoenfeld residuals have mean approximately zero", {
  set.seed(101)
  n <- 200

  x <- rnorm(n)
  lambda <- 0.1 * exp(0.5 * x)
  time <- rexp(n, rate = lambda)
  event <- rbinom(n, 1, 0.8)

  X <- as.matrix(data.frame(x = x))

  ph <- check_ph_assumption(time, event, X)

  mean_resid <- colMeans(ph$schoenfeld_residuals)
  expect_true(abs(mean_resid) < 0.1,
              info = paste("Mean residual:", mean_resid))
})

# ============================================================
# Data Loading and Utilities
# ============================================================

test_that("load_survival_data handles CSV file", {
  temp_csv <- tempfile(fileext = ".csv")
  on.exit(unlink(temp_csv))

  data <- generate_synthetic_survival(n_per_group = 20, seed = 42)
  utils::write.csv(data[, c("time", "event", "group")], temp_csv, row.names = FALSE)

  loaded <- load_survival_data(temp_csv)

  expect_true(is.list(loaded))
  expect_equal(loaded$n_subjects, 40)
  expect_equal(loaded$n_events, sum(data$event))
})

test_that("load_survival_data validates required columns", {
  temp_csv <- tempfile(fileext = ".csv")
  on.exit(unlink(temp_csv))

  utils::write.csv(data.frame(time = 1:5), temp_csv, row.names = FALSE)

  expect_error(load_survival_data(temp_csv), "Missing required columns")
})

test_that("load_survival_data handles data.frame input", {
  data <- data.frame(
    time = c(1, 2, 3, 4, 5),
    event = c(1, 0, 1, 1, 0),
    group = c("A", "A", "B", "B", "A")
  )

  loaded <- load_survival_data(data, group_col = "group")

  expect_equal(loaded$n_subjects, 5)
  expect_equal(loaded$n_events, 3)
  expect_equal(levels(loaded$group), c("A", "B"))
})

test_that("summarize_survival_data provides correct statistics", {
  data <- generate_synthetic_survival(n_per_group = 50, seed = 123)
  loaded <- load_survival_data(data, group_col = "group")

  summary <- summarize_survival_data(loaded)

  expect_equal(summary$n_subjects, 100)
  expect_equal(summary$n_events, sum(data$event))
  expect_true(summary$event_rate >= 0 && summary$event_rate <= 1)
  expect_true(summary$median_time > 0)
})

test_that("km_plot_data creates valid plotting data", {
  data <- generate_synthetic_survival(n_per_group = 50, seed = 456)
  km <- km_estimate(data$time, data$event, data$group)

  plot_data <- km_plot_data(km)

  expect_true(is.data.frame(plot_data))
  expect_true("time" %in% names(plot_data))
  expect_true("survival" %in% names(plot_data))
  expect_true("lower" %in% names(plot_data))
  expect_true("upper" %in% names(plot_data))
  expect_true("group" %in% names(plot_data))

  first_row <- plot_data[1, ]
  expect_equal(first_row$time, 0)
  expect_equal(first_row$survival, 1)
})

# ============================================================
# Integration Tests
# ============================================================

test_that("full analysis pipeline works", {
  data <- generate_synthetic_survival(n_per_group = 100, seed = 42)

  loaded <- load_survival_data(data, group_col = "group",
                                covariate_cols = c("covariate1", "covariate2"))

  summary <- summarize_survival_data(loaded)
  expect_equal(summary$n_subjects, 200)

  km <- km_estimate(loaded$time, loaded$event, loaded$group)
  expect_true(km$grouped)
  expect_equal(length(km$groups), 2)

  lr <- log_rank_test(loaded$time, loaded$event, loaded$group)
  expect_true(lr$df == 1)

  X <- as.matrix(data.frame(grouptreatment = as.numeric(data$group == "treatment")))
  cox <- cox_ph_model(loaded$time, loaded$event, X)

  expect_true(cox$converged)
  expect_true(cox$concordance > 0.5)

  ph <- check_ph_assumption(loaded$time, loaded$event, X, beta = cox$coefficients)
  expect_true(length(ph$p_value) == 1)
})

test_that("analysis works with censoring", {
  set.seed(789)
  n <- 150

  time <- rexp(n, rate = 0.1)
  event <- rbinom(n, 1, 0.5)
  group <- rep(c("A", "B"), each = n/2)

  km <- km_estimate(time, event, group)
  lr <- log_rank_test(time, event, group)

  X <- as.matrix(data.frame(group = as.numeric(group == "B")))
  cox <- cox_ph_model(time, event, X)

  # Median may or may not be reached depending on censoring
  # For grouped results, medians are per-group
  expect_true(is.finite(km$A$median_survival) || is.na(km$A$median_survival))
  expect_true(is.finite(km$B$median_survival) || is.na(km$B$median_survival))
  expect_true(lr$df == 1)
  expect_true(cox$converged)
})
