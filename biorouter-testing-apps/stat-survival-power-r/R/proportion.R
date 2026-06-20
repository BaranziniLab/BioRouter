#' Power and Sample Size for Two-Proportion Tests
#'
#' Uses the normal approximation (arc-sine or unpooled) to the difference
#' in proportions.
#'
#' @name proportion_power
NULL

# ---------------------------------------------------------------------------
# Internal
# ---------------------------------------------------------------------------

# Unpooled standard error of the difference in proportions
.se_diff_prop <- function(p1, p2, n) {
  sqrt(p1 * (1 - p1) / n + p2 * (1 - p2) / n)
}

# ---------------------------------------------------------------------------
# Exported functions
# ---------------------------------------------------------------------------

#' Compute power for a two-proportion z-test
#'
#' @param n Sample size per group.
#' @param p1 Proportion in group 1.
#' @param p2 Proportion in group 2.
#' @param alpha Significance level (default 0.05).
#' @return Power.
#' @export
#' @examples
#' power_two_proportion(n = 100, p1 = 0.30, p2 = 0.50)
power_two_proportion <- function(n, p1, p2, alpha = 0.05) {
  se <- .se_diff_prop(p1, p2, n)
  diff <- abs(p1 - p2)
  z_crit <- qnorm(1 - alpha / 2)
  # Non-centrality parameter
  ncp <- diff / se
  # Power = P(Z > z_crit - ncp) + P(Z < -z_crit - ncp)
  power <- pnorm(z_crit - ncp, lower.tail = FALSE) +
           pnorm(-z_crit - ncp, lower.tail = TRUE)
  power
}

#' Compute required sample size per group for a two-proportion test
#'
#' @param p1 Proportion in group 1.
#' @param p2 Proportion in group 2.
#' @param power Desired power.
#' @param alpha Significance level.
#' @return Named list with \code{n} (per group) and \code{achieved_power}.
#' @export
#' @examples
#' sample_size_two_proportion(p1 = 0.30, p2 = 0.50, power = 0.80)
sample_size_two_proportion <- function(p1, p2, power = 0.80, alpha = 0.05) {
  lo <- 2L
  hi <- 1000000L
  if (power_two_proportion(lo, p1, p2, alpha) >= power) {
    return(list(n = lo, achieved_power = power_two_proportion(lo, p1, p2, alpha)))
  }
  while (hi - lo > 1L) {
    mid <- (lo + hi) %/% 2L
    pw <- power_two_proportion(mid, p1, p2, alpha)
    if (pw >= power) hi <- mid else lo <- mid
  }
  list(n = hi, achieved_power = power_two_proportion(hi, p1, p2, alpha))
}
