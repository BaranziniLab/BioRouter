#' Power and Sample Size for Correlation Tests
#'
#' Tests H0: rho = 0 using the Fisher z transformation.
#'
#' @name correlation_power
NULL

# ---------------------------------------------------------------------------
# Exported functions
# ---------------------------------------------------------------------------

#' Compute power for a correlation test
#'
#' Uses Fisher's z-transformation. Power is computed as the probability
#' that the transformed sample correlation exceeds the critical value
#' under H1.
#'
#' @param n Sample size.
#' @param r Population correlation coefficient.
#' @param alpha Significance level (default 0.05).
#' @return Power.
#' @export
#' @examples
#' power_correlation(n = 50, r = 0.3)
power_correlation <- function(n, r, alpha = 0.05) {
  # Fisher z-transform of r
  z_r <- 0.5 * log((1 + r) / (1 - r))
  # Standard error under H1
  se <- 1 / sqrt(n - 3)
  # Critical value under H0 (rho = 0, z_0 = 0)
  z_crit <- qnorm(1 - alpha / 2) * se  # This is in z-scale
  # Wait — under H0, z ~ N(0, 1/sqrt(n-3)). So z_crit_H0 = qnorm(1-alpha/2) / sqrt(n-3)
  z_crit_h0 <- qnorm(1 - alpha / 2) / sqrt(n - 3)
  # Power: P(|z_r| > z_crit_h0) = P(z_r > z_crit_h0) + P(z_r < -z_crit_h0)
  power <- pnorm(z_r, mean = z_r, sd = se, lower.tail = FALSE) +
           pnorm(-z_r, mean = z_r, sd = se, lower.tail = TRUE)
  # Actually simpler: z_r ~ N(z_rho, se). Reject if |z_sample| > z_crit_h0
  # But z_sample ~ N(z_rho, se). So power = P(z_sample > z_crit_h0 | H1) + P(z_sample < -z_crit_h0 | H1)
  # Under H1: z_sample ~ N(z_r, se)
  power <- pnorm(z_crit_h0, mean = z_r, sd = se, lower.tail = FALSE) +
           pnorm(-z_crit_h0, mean = z_r, sd = se, lower.tail = TRUE)
  power
}

#' Compute required sample size for a correlation test
#'
#' @param r Population correlation.
#' @param power Desired power.
#' @param alpha Significance level.
#' @return Named list with \code{n} and \code{achieved_power}.
#' @export
#' @examples
#' sample_size_correlation(r = 0.3, power = 0.80)
sample_size_correlation <- function(r, power = 0.80, alpha = 0.05) {
  lo <- 5L
  hi <- 100000L
  if (power_correlation(lo, r, alpha) >= power) {
    return(list(n = lo, achieved_power = power_correlation(lo, r, alpha)))
  }
  while (hi - lo > 1L) {
    mid <- (lo + hi) %/% 2L
    pw <- power_correlation(mid, r, alpha)
    if (pw >= power) hi <- mid else lo <- mid
  }
  list(n = hi, achieved_power = power_correlation(hi, r, alpha))
}
