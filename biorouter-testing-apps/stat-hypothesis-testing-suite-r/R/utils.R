#' Core utilities for the hypothesis testing suite.
#' Provides tidy result formatting, effect size calculations, and CI helpers.

#' Create a tidy test result
#'
#' @param test_name Character: name of the test
#' @param statistic Numeric: test statistic value
#' @param df Numeric or character: degrees of freedom
#' @param p_value Numeric: p-value
#' @param effect_size Numeric or NULL: effect size estimate
#' @param effect_name Character or NULL: name of effect size measure
#' @param ci_lower Numeric or NULL: lower bound of confidence interval
#' @param ci_upper Numeric or NULL: upper bound of confidence interval
#' @param alternative Character: "two.sided", "less", or "greater"
#' @param method Character: description of the test method
#' @param data_name Character or NULL: name of the data input
#' @param extra Named list of additional result fields (optional)
#' @return A named list of class "hyp_result" with tidy test results
#' @export
tidy_result <- function(test_name, statistic, df, p_value,
                        effect_size = NULL, effect_name = NULL,
                        ci_lower = NULL, ci_upper = NULL,
                        alternative = "two.sided", method = "",
                        data_name = NULL, extra = list()) {
  # Ensure p-value is in [0, 1]
  p_value <- max(0, min(1, p_value))

  result <- list(
    test_name = test_name,
    statistic = statistic,
    df = df,
    p_value = p_value,
    effect_size = effect_size,
    effect_name = effect_name,
    ci_lower = ci_lower,
    ci_upper = ci_upper,
    alternative = alternative,
    method = method,
    data_name = data_name,
    extra = extra,
    significant = p_value < 0.05
  )
  class(result) <- "hyp_result"
  return(result)
}

#' Print method for hyp_result objects
#' @export
print.hyp_result <- function(x, ...) {
  cat(sprintf("=== %s ===\n", x$test_name))
  if (nchar(x$method) > 0) cat(sprintf("Method: %s\n", x$method))
  cat(sprintf("Statistic: %.6f\n", x$statistic))
  cat(sprintf("df: %s\n", paste(x$df, collapse = ", ")))
  cat(sprintf("p-value: %.6f\n", x$p_value))
  if (!is.null(x$effect_size)) {
    cat(sprintf("%s: %.6f\n", x$effect_name %||% "Effect size", x$effect_size))
  }
  if (!is.null(x$ci_lower) && !is.null(x$ci_upper)) {
    cat(sprintf("95%% CI: [%.6f, %.6f]\n", x$ci_lower, x$ci_upper))
  }
  cat(sprintf("Alternative: %s\n", x$alternative))
  cat(sprintf("Significant at alpha=0.05: %s\n",
              ifelse(x$significant, "YES", "NO")))
  if (length(x$extra) > 0) {
    for (nm in names(x$extra)) {
      cat(sprintf("%s: %s\n", nm, x$extra[[nm]]))
    }
  }
  invisible(x)
}

#' Null coalescing operator
#' @export
`%||%` <- function(a, b) {
  if (!is.null(a)) a else b
}

# ---- Effect Size Functions ----

#' Cohen's d for one-sample or two-sample comparisons
#'
#' @param x Numeric vector (or second group for two-sample)
#' @param y Numeric vector or NULL (for one-sample)
#' @param mu Numeric: hypothesized mean for one-sample (default 0)
#' @return Numeric Cohen's d value
#' @export
effects_cohens_d <- function(x, y = NULL, mu = 0) {
  if (is.null(y)) {
    # One-sample
    n <- length(x)
    s <- sd(x)
    d <- (mean(x) - mu) / s
    # Hedges g correction: multiply by (1 - 3/(4*n - 1))
    # But pure Cohen's d does not apply correction
    return(d)
  } else {
    # Two-sample (pooled sd)
    n1 <- length(x)
    n2 <- length(y)
    s1 <- sd(x)
    s2 <- sd(y)
    sp <- sqrt(((n1 - 1) * s1^2 + (n2 - 1) * s2^2) / (n1 + n2 - 2))
    d <- (mean(x) - mean(y)) / sp
    return(d)
  }
}

#' Hedges' g (bias-corrected Cohen's d)
#'
#' @param x Numeric vector (or second group for two-sample)
#' @param y Numeric vector or NULL (for one-sample)
#' @return Numeric Hedges' g value
#' @export
effects_hedges_g <- function(x, y = NULL) {
  d <- effects_cohens_d(x, y)
  if (is.null(y)) {
    n <- length(x)
  } else {
    n <- length(x) + length(y)
  }
  # Hedges' correction factor
  correction <- 1 - 3 / (4 * (n - 1) - 1)
  return(d * correction)
}

#' Omega squared (omega-sq) for one-way ANOVA
#'
#' @param ss_between Numeric: between-group sum of squares
#' @param ss_within Numeric: within-group (error) sum of squares
#' @param df_between Numeric: between-group df
#' @param df_within Numeric: within-group df
#' @param n_total Numeric: total number of observations
#' @return Numeric omega-squared value
#' @export
effects_omega_squared <- function(ss_between, ss_within, df_between, df_within, n_total) {
  ms_between <- ss_between / df_between
  ms_within <- ss_within / df_within
  omega2 <- (ss_between - df_between * ms_within) /
    (ss_total(ss_between, ss_within) + ms_within)
  return(max(0, omega2))  # omega-sq is bounded below by 0
}

#' Eta squared for ANOVA
#'
#' @param ss_effect Numeric: sum of squares for the effect
#' @param ss_total Numeric: total sum of squares
#' @return Numeric eta-squared value
#' @export
effects_eta_squared <- function(ss_effect, ss_total) {
  return(ss_effect / ss_total)
}

#' Epsilon squared (bias-corrected eta-squared)
#'
#' @param eta_sq Numeric: eta-squared value
#' @param df_effect Numeric: df for the effect
#' @param df_total Numeric: total df
#' @return Numeric epsilon-squared value
#' @export
effects_epsilon_squared <- function(eta_sq, df_effect, df_total) {
  k <- df_effect + 1  # number of groups
  n <- df_total + 1   # total observations
  epsilon2 <- eta_sq - (df_effect * (1 - eta_sq)) / (n - df_effect)
  return(epsilon2)
}

# Internal helper
ss_total <- function(ss_between, ss_within) {
  return(ss_between + ss_within)
}

# ---- Confidence Interval Functions ----

#' Confidence interval for a mean (t-based)
#'
#' @param x Numeric vector of data
#' @param conf_level Confidence level (default 0.95)
#' @return Named list with lower, upper, margin
#' @export
ci_t_mean <- function(x, conf_level = 0.95) {
  n <- length(x)
  m <- mean(x)
  se <- sd(x) / sqrt(n)
  t_crit <- qt((1 + conf_level) / 2, df = n - 1)
  margin <- t_crit * se
  return(list(lower = m - margin, upper = m + margin, margin = margin))
}

#' Confidence interval for a correlation coefficient (Fisher z-transform)
#'
#' @param r Numeric: sample correlation
#' @param n Integer: sample size
#' @param conf_level Confidence level (default 0.95)
#' @return Named list with lower, upper
#' @export
ci_correlation <- function(r, n, conf_level = 0.95) {
  # Fisher z-transform
  z <- 0.5 * log((1 + r) / (1 - r))
  se_z <- 1 / sqrt(n - 3)
  z_crit <- qnorm((1 + conf_level) / 2)
  # CI on z scale
  z_lower <- z - z_crit * se_z
  z_upper <- z + z_crit * se_z
  # Back-transform
  lower <- (exp(2 * z_lower) - 1) / (exp(2 * z_lower) + 1)
  upper <- (exp(2 * z_upper) - 1) / (exp(2 * z_upper) + 1)
  return(list(lower = lower, upper = upper))
}

#' Confidence interval for a proportion (Wilson score)
#'
#' @param p_hat Numeric: sample proportion
#' @param n Integer: sample size
#' @param conf_level Confidence level (default 0.95)
#' @return Named list with lower, upper
#' @export
ci_proportion <- function(p_hat, n, conf_level = 0.95) {
  z <- qnorm((1 + conf_level) / 2)
  denom <- 1 + z^2 / n
  center <- (p_hat + z^2 / (2 * n)) / denom
  spread <- z * sqrt((p_hat * (1 - p_hat) / n + z^2 / (4 * n^2)) / denom)
  return(list(lower = center - spread, upper = center + spread))
}
