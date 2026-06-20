#' Parametric Hypothesis Tests
#'
#' Implements t-tests, ANOVA, F-test, Pearson correlation, and linear regression
#' from scratch, returning tidy results validated against base R.

# ---- One-Sample t-test ----

#' One-sample t-test (implemented from scratch)
#'
#' @param x Numeric vector of data
#' @param mu Numeric: hypothesized population mean (default 0)
#' @param alternative Character: "two.sided", "less", or "greater"
#' @param conf_level Numeric: confidence level for CI (default 0.95)
#' @return A tidy_result object
#' @export
hyp_one_sample_t <- function(x, mu = 0, alternative = "two.sided", conf_level = 0.95) {
  x <- x[!is.na(x)]
  n <- length(x)
  if (n < 2) stop("Need at least 2 observations")

  m <- mean(x)
  s <- sd(x)
  se <- s / sqrt(n)
  t_stat <- (m - mu) / se
  df <- n - 1

  # p-value from t-distribution
  if (alternative == "two.sided") {
    p_val <- 2 * (1 - t_cdf(abs(t_stat), df))
  } else if (alternative == "less") {
    p_val <- t_cdf(t_stat, df)
  } else {
    p_val <- 1 - t_cdf(t_stat, df)
  }

  # Effect size: Cohen's d
  d <- (m - mu) / s

  # CI for the mean
  t_crit <- qt((1 + conf_level) / 2, df)
  margin <- t_crit * se
  ci_lower <- m - margin
  ci_upper <- m + margin

  return(tidy_result(
    test_name = "One-Sample t-test",
    statistic = t_stat,
    df = df,
    p_value = p_val,
    effect_size = d,
    effect_name = "Cohen's d",
    ci_lower = ci_lower,
    ci_upper = ci_upper,
    alternative = alternative,
    method = "One-sample t-test (from scratch)",
    extra = list(mean = m, sd = s, se = se, n = n, mu = mu)
  ))
}

# ---- Two-Sample t-test ----

#' Two-sample t-test (equal variances assumed)
#'
#' @param x Numeric vector (group 1)
#' @param y Numeric vector (group 2)
#' @param alternative Character: "two.sided", "less", or "greater"
#' @param conf_level Numeric: confidence level for CI (default 0.95)
#' @return A tidy_result object
#' @export
hyp_two_sample_t <- function(x, y, alternative = "two.sided", conf_level = 0.95) {
  x <- x[!is.na(x)]
  y <- y[!is.na(y)]
  n1 <- length(x)
  n2 <- length(y)
  if (n1 < 2 || n2 < 2) stop("Each group needs at least 2 observations")

  m1 <- mean(x)
  m2 <- mean(y)
  s1 <- var(x)
  s2 <- var(y)
  df <- n1 + n2 - 2
  sp <- sqrt(((n1 - 1) * s1 + (n2 - 1) * s2) / df)  # pooled sd
  se <- sp * sqrt(1/n1 + 1/n2)
  t_stat <- (m1 - m2) / se

  if (alternative == "two.sided") {
    p_val <- 2 * (1 - t_cdf(abs(t_stat), df))
  } else if (alternative == "less") {
    p_val <- t_cdf(t_stat, df)
  } else {
    p_val <- 1 - t_cdf(t_stat, df)
  }

  # Cohen's d
  d <- effects_cohens_d(x, y)

  # CI for difference in means
  t_crit <- qt((1 + conf_level) / 2, df)
  margin <- t_crit * se
  diff <- m1 - m2

  return(tidy_result(
    test_name = "Two-Sample t-test",
    statistic = t_stat,
    df = df,
    p_value = p_val,
    effect_size = d,
    effect_name = "Cohen's d",
    ci_lower = diff - margin,
    ci_upper = diff + margin,
    alternative = alternative,
    method = "Two-sample t-test with equal variances (from scratch)",
    extra = list(mean1 = m1, mean2 = m2, diff = diff, sp = sp,
                 n1 = n1, n2 = n2)
  ))
}

# ---- Paired t-test ----

#' Paired samples t-test
#'
#' @param x Numeric vector (pre/test scores)
#' @param y Numeric vector (post/control scores)
#' @param alternative Character: "two.sided", "less", or "greater"
#' @param conf_level Numeric: confidence level for CI (default 0.95)
#' @return A tidy_result object
#' @export
hyp_paired_t <- function(x, y, alternative = "two.sided", conf_level = 0.95) {
  if (length(x) != length(y)) stop("x and y must have the same length")
  d <- x - y
  d <- d[!is.na(d)]
  n <- length(d)
  if (n < 2) stop("Need at least 2 paired observations")

  m_d <- mean(d)
  s_d <- sd(d)
  se <- s_d / sqrt(n)
  t_stat <- m_d / se
  df <- n - 1

  if (alternative == "two.sided") {
    p_val <- 2 * (1 - t_cdf(abs(t_stat), df))
  } else if (alternative == "less") {
    p_val <- t_cdf(t_stat, df)
  } else {
    p_val <- 1 - t_cdf(t_stat, df)
  }

  # Cohen's d for paired
  d_effect <- m_d / s_d

  t_crit <- qt((1 + conf_level) / 2, df)
  margin <- t_crit * se

  return(tidy_result(
    test_name = "Paired t-test",
    statistic = t_stat,
    df = df,
    p_value = p_val,
    effect_size = d_effect,
    effect_name = "Cohen's d (paired)",
    ci_lower = m_d - margin,
    ci_upper = m_d + margin,
    alternative = alternative,
    method = "Paired samples t-test (from scratch)",
    extra = list(mean_diff = m_d, sd_diff = s_d, n = n)
  ))
}

# ---- Welch's t-test ----

#' Welch's t-test (does not assume equal variances)
#'
#' @param x Numeric vector (group 1)
#' @param y Numeric vector (group 2)
#' @param alternative Character: "two.sided", "less", or "greater"
#' @param conf_level Numeric: confidence level (default 0.95)
#' @return A tidy_result object
#' @export
hyp_welch_t <- function(x, y, alternative = "two.sided", conf_level = 0.95) {
  x <- x[!is.na(x)]
  y <- y[!is.na(y)]
  n1 <- length(x)
  n2 <- length(y)
  if (n1 < 2 || n2 < 2) stop("Each group needs at least 2 observations")

  m1 <- mean(x)
  m2 <- mean(y)
  v1 <- var(x)
  v2 <- var(y)
  se <- sqrt(v1/n1 + v2/n2)
  t_stat <- (m1 - m2) / se

  # Welch-Satterthwaite df
  num <- (v1/n1 + v2/n2)^2
  denom <- (v1/n1)^2 / (n1 - 1) + (v2/n2)^2 / (n2 - 1)
  df <- num / denom

  if (alternative == "two.sided") {
    p_val <- 2 * (1 - t_cdf(abs(t_stat), df))
  } else if (alternative == "less") {
    p_val <- t_cdf(t_stat, df)
  } else {
    p_val <- 1 - t_cdf(t_stat, df)
  }

  # Cohen's d (using pooled SD from equal-variance version)
  d <- effects_cohens_d(x, y)

  t_crit <- qt((1 + conf_level) / 2, df)
  margin <- t_crit * se
  diff <- m1 - m2

  return(tidy_result(
    test_name = "Welch's t-test",
    statistic = t_stat,
    df = df,
    p_value = p_val,
    effect_size = d,
    effect_name = "Cohen's d",
    ci_lower = diff - margin,
    ci_upper = diff + margin,
    alternative = alternative,
    method = "Welch two-sample t-test (from scratch)",
    extra = list(mean1 = m1, mean2 = m2, diff = diff, n1 = n1, n2 = n2)
  ))
}

# ---- One-Way ANOVA ----

#' One-way ANOVA (implemented from scratch)
#'
#' @param formula Formula of the form y ~ group
#' @param data A data frame containing the variables
#' @return A tidy_result object
#' @export
hyp_one_way_anova <- function(formula, data) {
  mf <- model.frame(formula, data = data)
  y <- model.response(mf)
  groups <- mf[, 2]
  group_levels <- unique(groups)
  k <- length(group_levels)
  n <- length(y)

  if (k < 2) stop("Need at least 2 groups")
  if (n <= k) stop("Need more observations than groups")

  grand_mean <- mean(y)

  # Compute sums of squares
  ss_between <- 0
  ss_within <- 0
  group_means <- numeric(k)
  group_ns <- numeric(k)

  for (i in seq_along(group_levels)) {
    gi <- groups == group_levels[i]
    yi <- y[gi]
    ni <- length(yi)
    mi <- mean(yi)
    group_means[i] <- mi
    group_ns[i] <- ni
    ss_between <- ss_between + ni * (mi - grand_mean)^2
    ss_within <- ss_within + sum((yi - mi)^2)
  }

  ss_total <- ss_between + ss_within
  df_between <- k - 1
  df_within <- n - k
  df_total <- n - 1

  ms_between <- ss_between / df_between
  ms_within <- ss_within / df_within

  f_stat <- ms_between / ms_within
  p_val <- 1 - f_cdf(f_stat, df_between, df_within)

  # Effect sizes
  eta2 <- effects_eta_squared(ss_between, ss_total)
  omega2 <- effects_omega_squared(ss_between, ss_within, df_between, df_within, n)
  epsilon2 <- effects_epsilon_squared(eta2, df_between, df_total)

  return(tidy_result(
    test_name = "One-Way ANOVA",
    statistic = f_stat,
    df = c(df_between, df_within),
    p_value = p_val,
    effect_size = eta2,
    effect_name = "eta-squared",
    alternative = "greater",
    method = "One-way ANOVA (from scratch)",
    extra = list(
      ss_between = ss_between, ss_within = ss_within, ss_total = ss_total,
      ms_between = ms_between, ms_within = ms_within,
      omega_squared = omega2, epsilon_squared = epsilon2,
      n_groups = k, n_total = n,
      group_means = group_means, group_ns = group_ns
    )
  ))
}

# ---- Two-Way ANOVA ----

#' Two-way ANOVA (main effects only, no interaction)
#'
#' @param formula Formula of the form y ~ factor1 + factor2
#' @param data A data frame
#' @return A list of tidy_result objects for factor1, factor2, and residuals
#' @export
hyp_two_way_anova <- function(formula, data) {
  mf <- model.frame(formula, data = data)
  y <- model.response(mf)
  factor1 <- mf[, 2]
  factor2 <- mf[, 3]

  n <- length(y)
  grand_mean <- mean(y)

  # Factor 1
  levels1 <- unique(factor1)
  k1 <- length(levels1)
  ss_f1 <- 0
  for (lev in levels1) {
    idx <- factor1 == lev
    ni <- sum(idx)
    ss_f1 <- ss_f1 + ni * (mean(y[idx]) - grand_mean)^2
  }
  df_f1 <- k1 - 1

  # Factor 2
  levels2 <- unique(factor2)
  k2 <- length(levels2)
  ss_f2 <- 0
  for (lev in levels2) {
    idx <- factor2 == lev
    ni <- sum(idx)
    ss_f2 <- ss_f2 + ni * (mean(y[idx]) - grand_mean)^2
  }
  df_f2 <- k2 - 1

  # Within (error) - compute cell means for cell means model
  ss_total <- sum((y - grand_mean)^2)
  ss_model <- ss_f1 + ss_f2
  ss_error <- ss_total - ss_model
  df_error <- n - k1 - k2

  ms_f1 <- ss_f1 / df_f1
  ms_f2 <- ss_f2 / df_f2
  ms_error <- ss_error / df_error

  f1 <- ms_f1 / ms_error
  f2 <- ms_f2 / ms_error
  p1 <- 1 - f_cdf(f1, df_f1, df_error)
  p2 <- 1 - f_cdf(f2, df_f2, df_error)

  eta1 <- effects_eta_squared(ss_f1, ss_total)
  eta2 <- effects_eta_squared(ss_f2, ss_total)

  result1 <- tidy_result(
    test_name = "Two-Way ANOVA - Factor 1",
    statistic = f1, df = c(df_f1, df_error), p_value = p1,
    effect_size = eta1, effect_name = "eta-squared",
    method = "Two-way ANOVA (from scratch)",
    extra = list(ss = ss_f1, ms = ms_f1)
  )

  result2 <- tidy_result(
    test_name = "Two-Way ANOVA - Factor 2",
    statistic = f2, df = c(df_f2, df_error), p_value = p2,
    effect_size = eta2, effect_name = "eta-squared",
    method = "Two-way ANOVA (from scratch)",
    extra = list(ss = ss_f2, ms = ms_f2)
  )

  return(list(factor1 = result1, factor2 = result2,
              ss_total = ss_total, ss_error = ss_error,
              df_error = df_error))
}

# ---- F-test for Equality of Variances ----

#' F-test for comparing two variances
#'
#' @param x Numeric vector (group 1)
#' @param y Numeric vector (group 2)
#' @param alternative Character: "two.sided", "less", or "greater"
#' @return A tidy_result object
#' @export
hyp_f_test_variances <- function(x, y, alternative = "two.sided") {
  x <- x[!is.na(x)]
  y <- y[!is.na(y)]
  n1 <- length(x)
  n2 <- length(y)
  if (n1 < 2 || n2 < 2) stop("Each group needs at least 2 observations")

  v1 <- var(x)
  v2 <- var(y)
  f_stat <- v1 / v2
  df1 <- n1 - 1
  df2 <- n2 - 1

  if (alternative == "two.sided") {
    p_val <- 2 * min(1 - f_cdf(f_stat, df1, df2),
                      f_cdf(f_stat, df1, df2))
  } else if (alternative == "greater") {
    p_val <- 1 - f_cdf(f_stat, df1, df2)
  } else {
    p_val <- f_cdf(f_stat, df1, df2)
  }

  return(tidy_result(
    test_name = "F-test for Variances",
    statistic = f_stat,
    df = c(df1, df2),
    p_value = p_val,
    effect_size = v1 / v2,
    effect_name = "Variance ratio",
    alternative = alternative,
    method = "F-test for equality of two variances (from scratch)",
    extra = list(var1 = v1, var2 = v2, n1 = n1, n2 = n2)
  ))
}

# ---- Pearson Correlation Test ----

#' Pearson correlation coefficient test
#'
#' @param x Numeric vector
#' @param y Numeric vector
#' @param alternative Character: "two.sided", "less", or "greater"
#' @param conf_level Numeric: confidence level for CI (default 0.95)
#' @return A tidy_result object
#' @export
hyp_pearson_r <- function(x, y, alternative = "two.sided", conf_level = 0.95) {
  # Remove NAs pairwise
  complete <- complete.cases(x, y)
  x <- x[complete]
  y <- y[complete]
  n <- length(x)
  if (n < 3) stop("Need at least 3 paired observations")

  m_x <- mean(x)
  m_y <- mean(y)

  # Pearson r
  num <- sum((x - m_x) * (y - m_y))
  den <- sqrt(sum((x - m_x)^2) * sum((y - m_y)^2))
  r <- num / den

  # t-test for correlation
  t_stat <- r * sqrt((n - 2) / (1 - r^2))
  df <- n - 2

  if (alternative == "two.sided") {
    p_val <- 2 * (1 - t_cdf(abs(t_stat), df))
  } else if (alternative == "less") {
    p_val <- t_cdf(t_stat, df)
  } else {
    p_val <- 1 - t_cdf(t_stat, df)
  }

  # CI via Fisher z
  ci <- ci_correlation(r, n, conf_level)

  # R-squared as effect size
  r_squared <- r^2

  return(tidy_result(
    test_name = "Pearson Correlation Test",
    statistic = r,
    df = df,
    p_value = p_val,
    effect_size = r,
    effect_name = "r",
    ci_lower = ci$lower,
    ci_upper = ci$upper,
    alternative = alternative,
    method = "Pearson product-moment correlation (from scratch)",
    extra = list(r_squared = r_squared, n = n, t_stat_for_r = t_stat)
  ))
}

# ---- Simple Linear Regression ----

#' Simple linear regression with coefficient tests
#'
#' @param x Numeric vector (predictor)
#' @param y Numeric vector (response)
#' @return A tidy_result object with detailed regression output
#' @export
hyp_simple_regression <- function(x, y) {
  complete <- complete.cases(x, y)
  x <- x[complete]
  y <- y[complete]
  n <- length(x)
  if (n < 3) stop("Need at least 3 observations")

  m_x <- mean(x)
  m_y <- mean(y)

  ss_xx <- sum((x - m_x)^2)
  ss_yy <- sum((y - m_y)^2)
  ss_xy <- sum((x - m_x) * (y - m_y))

  beta1 <- ss_xy / ss_xx
  beta0 <- m_y - beta1 * m_x

  # Fitted values and residuals
  y_hat <- beta0 + beta1 * x
  residuals <- y - y_hat
  ss_res <- sum(residuals^2)
  ss_reg <- ss_yy - ss_res
  df_reg <- 1
  df_res <- n - 2

  ms_reg <- ss_reg / df_reg
  ms_res <- ss_res / df_res
  f_stat <- ms_reg / ms_res
  p_val <- 1 - f_cdf(f_stat, df_reg, df_res)

  # Standard errors for coefficients
  se_beta1 <- sqrt(ms_res / ss_xx)
  se_beta0 <- sqrt(ms_res * (1/n + m_x^2 / ss_xx))

  # t-tests for coefficients
  t_beta1 <- beta1 / se_beta1
  t_beta0 <- beta0 / se_beta0
  p_beta1 <- 2 * (1 - t_cdf(abs(t_beta1), df_res))
  p_beta0 <- 2 * (1 - t_cdf(abs(t_beta0), df_res))

  # R-squared
  r_squared <- ss_reg / ss_yy
  adj_r_squared <- 1 - (1 - r_squared) * (n - 1) / df_res

  # CIs for beta1
  t_crit <- qt(0.975, df_res)
  ci_beta1_lower <- beta1 - t_crit * se_beta1
  ci_beta1_upper <- beta1 + t_crit * se_beta1

  return(tidy_result(
    test_name = "Simple Linear Regression",
    statistic = f_stat,
    df = c(df_reg, df_res),
    p_value = p_val,
    effect_size = r_squared,
    effect_name = "R-squared",
    ci_lower = ci_beta1_lower,
    ci_upper = ci_beta1_upper,
    method = "Simple linear regression (from scratch)",
    extra = list(
      beta0 = beta0, beta1 = beta1,
      se_beta0 = se_beta0, se_beta1 = se_beta1,
      t_beta0 = t_beta0, t_beta1 = t_beta1,
      p_beta0 = p_beta0, p_beta1 = p_beta1,
      r_squared = r_squared, adj_r_squared = adj_r_squared,
      ms_reg = ms_reg, ms_res = ms_res,
      n = n
    )
  ))
}

# ---- Multiple Linear Regression ----

#' Multiple linear regression with coefficient tests
#'
#' @param formula Formula of the form y ~ x1 + x2 + ...
#' @param data A data frame
#' @return A tidy_result object with overall F-test and coefficient table
#' @export
hyp_multiple_regression <- function(formula, data) {
  mf <- model.frame(formula, data = data)
  y <- model.response(mf)
  X <- model.matrix(formula, data = data)

  n <- length(y)
  p <- ncol(X)  # includes intercept
  df_reg <- p - 1
  df_res <- n - p

  if (n <= p) stop("Insufficient observations for regression")

  # OLS: beta = (X'X)^{-1} X'y
  XtX <- crossprod(X)
  Xty <- crossprod(X, y)
  beta <- solve(XtX, Xty)

  y_hat <- X %*% beta
  residuals <- as.vector(y - y_hat)
  ss_res <- sum(residuals^2)
  ss_reg <- sum((y_hat - mean(y))^2)
  ss_total <- ss_reg + ss_res

  ms_reg <- ss_reg / df_reg
  ms_res <- ss_res / df_res

  f_stat <- ms_reg / ms_res
  p_val <- 1 - f_cdf(f_stat, df_reg, df_res)

  # Covariance matrix of beta
  var_beta <- ms_res * solve(XtX)
  se_beta <- sqrt(diag(var_beta))
  t_beta <- beta / se_beta
  p_beta <- 2 * sapply(abs(t_beta), function(t) 1 - t_cdf(t, df_res))

  # R-squared
  r_squared <- ss_reg / ss_total
  adj_r_squared <- 1 - (1 - r_squared) * (n - 1) / df_res

  # Coefficient CIs
  t_crit <- qt(0.975, df_res)
  ci_lower <- beta - t_crit * se_beta
  ci_upper <- beta + t_crit * se_beta

  coef_names <- colnames(X)
  coef_table <- data.frame(
    coef = coef_names,
    estimate = beta,
    std_error = se_beta,
    t_value = t_beta,
    p_value = p_beta,
    ci_lower = ci_lower,
    ci_upper = ci_upper,
    stringsAsFactors = FALSE
  )

  return(tidy_result(
    test_name = "Multiple Linear Regression",
    statistic = f_stat,
    df = c(df_reg, df_res),
    p_value = p_val,
    effect_size = r_squared,
    effect_name = "R-squared",
    method = "Multiple linear regression (from scratch)",
    extra = list(
      coef_table = coef_table,
      r_squared = r_squared,
      adj_r_squared = adj_r_squared,
      df_reg = df_reg,
      df_res = df_res,
      n = n,
      p_predictors = df_reg
    )
  ))
}
