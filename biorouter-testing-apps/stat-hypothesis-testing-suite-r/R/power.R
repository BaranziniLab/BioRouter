#' Power Analysis and Sample Size Calculations
#'
#' Provides power and sample-size helpers for common tests.

#' Power calculation for t-tests
#'
#' @param n Sample size per group (or total for one-sample)
#' @param d Effect size (Cohen's d)
#' @param alpha Significance level (default 0.05)
#' @param alternative Character: "two.sided" or "one.sided"
#' @param test_type Character: "one.sample", "two.sample", or "paired"
#' @return Named list with power, n, d, alpha, test_type
#' @export
power_t_test <- function(n, d, alpha = 0.05, alternative = "two.sided",
                         test_type = "two.sample") {
  if (test_type == "one.sample" || test_type == "paired") {
    df <- n - 1
    ncp <- d * sqrt(n)
  } else {
    df <- 2 * n - 2
    ncp <- d * sqrt(n / 2)
  }

  t_crit <- qt(1 - alpha / 2, df)  # two-sided critical value

  if (alternative == "one.sided") {
    t_crit <- qt(1 - alpha, df)
  }

  # Power = P(|T| > t_crit | H1 is true)
  # Using non-central t-distribution approximation
  # Approximate: P(T > t_crit | ncp) + P(T < -t_crit | ncp)
  power <- 1 - pt(t_crit - ncp, df) + pt(-t_crit - ncp, df)

  # For one-sided
  if (alternative == "one.sided") {
    power <- 1 - pt(t_crit - ncp, df)
  }

  return(list(
    power = max(0, min(1, power)),
    n = n,
    d = d,
    alpha = alpha,
    alternative = alternative,
    test_type = test_type,
    df = df,
    ncp = ncp
  ))
}

#' Sample size calculation for t-tests
#'
#' @param power Desired power (default 0.80)
#' @param d Effect size (Cohen's d)
#' @param alpha Significance level (default 0.05)
#' @param alternative Character: "two.sided" or "one.sided"
#' @param test_type Character: "one.sample", "two.sample", or "paired"
#' @return Named list with required n, power, d, alpha
#' @export
sample_size_t_test <- function(power = 0.80, d, alpha = 0.05,
                               alternative = "two.sided",
                               test_type = "two.sample") {
  # Use iterative search
  n <- 2
  while (TRUE) {
    result <- power_t_test(n, d, alpha, alternative, test_type)
    if (result$power >= power) break
    n <- n + 1
    if (n > 10000) stop("Could not find sufficient sample size")
  }

  return(list(
    n = n,
    power = result$power,
    d = d,
    alpha = alpha,
    alternative = alternative,
    test_type = test_type
  ))
}

#' Power calculation for one-way ANOVA
#'
#' @param n Sample size per group
#' @param k Number of groups
#' @param f Effect size (Cohen's f)
#' @param alpha Significance level (default 0.05)
#' @return Named list with power, parameters
#' @export
power_anova <- function(n, k, f, alpha = 0.05) {
  df1 <- k - 1
  df2 <- k * (n - 1)
  ncp <- n * k * f^2

  # Non-central F distribution approximation
  f_crit <- qf(1 - alpha, df1, df2)

  # Power using non-central F distribution
  # P(F > f_crit | H1) where F ~ ncf(df1, df2, ncp)
  # Use the pf function with ncp parameter
  power <- 1 - pf(f_crit, df1, df2, ncp = ncp)

  return(list(
    power = max(0, min(1, power)),
    n = n,
    k = k,
    f = f,
    alpha = alpha,
    df1 = df1,
    df2 = df2,
    ncp = ncp
  ))
}

#' Sample size calculation for one-way ANOVA
#'
#' @param power Desired power (default 0.80)
#' @param k Number of groups
#' @param f Effect size (Cohen's f)
#' @param alpha Significance level (default 0.05)
#' @return Named list with required n per group
#' @export
sample_size_anova <- function(power = 0.80, k, f, alpha = 0.05) {
  n <- 2
  while (TRUE) {
    result <- power_anova(n, k, f, alpha)
    if (result$power >= power) break
    n <- n + 1
    if (n > 10000) stop("Could not find sufficient sample size")
  }

  return(list(
    n_per_group = n,
    power = result$power,
    k = k,
    f = f,
    alpha = alpha,
    n_total = n * k
  ))
}
