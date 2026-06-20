#' Normality Tests
#'
#' Implements Shapiro-Wilk and Kolmogorov-Smirnov tests from scratch.

# ---- Shapiro-Wilk Test ----

#' Shapiro-Wilk test for normality (simplified implementation)
#'
#' Uses the approximation from Royston (1982) for the W statistic
#' and normal approximation for p-values.
#'
#' @param x Numeric vector
#' @return A tidy_result object
#' @export
hyp_shapiro_wilk <- function(x) {
  x <- x[!is.na(x)]
  n <- length(x)

  if (n < 3) stop("Need at least 3 observations")
  if (n > 5000) warning("Shapiro-Wilk approximation may be less accurate for n > 5000")

  # Sort data
  x_sorted <- sort(x)
  x_bar <- mean(x_sorted)

  # Compute W statistic (simplified algorithm)
  # Using the approximation from Royston (1982)
  s_sq <- sum((x_sorted - x_bar)^2)

  # Compute a_i coefficients (approximation)
  # For the Shapiro-Wilk test, we need order statistics of the normal
  m_vals <- qnorm((seq(1, n) - 0.375) / (n + 0.25))

  # a_i coefficients from Royston
  a <- numeric(n)
  for (i in 1:n) {
    # Royston's approximation for a_i
    m_sq_sum <- sum(m_vals^2)
    a[i] <- m_vals[i] / sqrt(m_sq_sum)
  }

  # W statistic
  num <- (sum(a * x_sorted))^2
  W <- num / s_sq

  # Ensure W is in valid range
  W <- max(0, min(1, W))

  # Approximate p-value using Royston's method
  # Transform W to approximate normal
  if (n <= 11) {
    # For small n, use a simpler approximation
    mu <- 0.2718 * n - 0.1479
    sigma <- exp(0.3842 * log(n) - 1.3642)
  } else {
    # For larger n, use logarithmic transformation
    mu <- -1.5861 - 0.31082 * log(n) - 0.08130 * (log(n))^2
    sigma <- exp(0.0050309 * n - 0.38003 * log(n) + 0.1433)
  }

  z <- (log(1 - W) - mu) / sigma
  p_val <- 1 - norm_cdf(z)

  # Handle edge cases
  p_val <- max(0, min(1, p_val))

  return(tidy_result(
    test_name = "Shapiro-Wilk Test",
    statistic = W,
    df = n,
    p_value = p_val,
    effect_size = NULL,
    effect_name = NULL,
    alternative = "less",
    method = "Shapiro-Wilk test for normality (from scratch)",
    extra = list(n = n, mu_normal = mu, sigma_normal = sigma)
  ))
}

# ---- Kolmogorov-Smirnov Test ----

#' Kolmogorov-Smirnov test for normality
#'
#' Tests if data comes from a normal distribution with specified parameters.
#' If mean/sd not provided, estimates from data.
#'
#' @param x Numeric vector
#' @param mu Numeric: hypothesized mean (default: estimated from data)
#' @param sigma Numeric: hypothesized sd (default: estimated from data)
#' @return A tidy_result object
#' @export
hyp_ks_test <- function(x, mu = NULL, sigma = NULL) {
  x <- x[!is.na(x)]
  n <- length(x)

  if (n < 1) stop("Need at least 1 observation")

  # Estimate parameters if not provided
  if (is.null(mu)) mu <- mean(x)
  if (is.null(sigma)) sigma <- sd(x)

  # Standardize
  x_std <- (x - mu) / sigma

  # ECDF values
  x_sorted <- sort(x_std)
  ecdf_vals <- (1:n) / n

  # Theoretical CDF (normal)
  cdf_vals <- norm_cdf(x_sorted)

  # KS statistic: D = max|F_n(x) - F(x)|
  # Check both sides
  d_plus <- max(ecdf_vals - cdf_vals)
  d_minus <- max(cdf_vals - (0:(n-1))/n)
  d_stat <- max(d_plus, d_minus)

  # P-value using Kolmogorov distribution approximation
  # P(D >= d) ≈ 2 * sum((-1)^(k+1) * exp(-2*k^2*lambda^2))
  # where lambda = (sqrt(n) + 0.12 + 0.11/sqrt(n)) * d
  lambda <- (sqrt(n) + 0.12 + 0.11/sqrt(n)) * d_stat

  p_val <- 0
  for (k in 1:100) {
    term <- 2 * (-1)^(k+1) * exp(-2 * k^2 * lambda^2)
    p_val <- p_val + term
    if (abs(term) < 1e-10) break
  }
  p_val <- max(0, min(1, p_val))

  return(tidy_result(
    test_name = "Kolmogorov-Smirnov Test",
    statistic = d_stat,
    df = n,
    p_value = p_val,
    effect_size = NULL,
    effect_name = NULL,
    alternative = "two.sided",
    method = "Kolmogorov-Smirnov test for normality (from scratch)",
    extra = list(
      n = n, mu = mu, sigma = sigma,
      D_plus = d_plus, D_minus = d_minus
    )
  ))
}
