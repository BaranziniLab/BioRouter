#' Power and Sample Size for Chi-Square Tests
#'
#' Uses the non-central chi-square distribution with effect size w.
#'
#' @name chi_square_power
NULL

# ---------------------------------------------------------------------------
# Exported functions
# ---------------------------------------------------------------------------

#' Compute power for a chi-square test of independence
#'
#' @param n Total sample size.
#' @param w Cohen's w effect size.
#' @param df Degrees of freedom (rows-1)*(cols-1).
#' @param alpha Significance level (default 0.05).
#' @return Power.
#' @export
#' @examples
#' power_chi_square(n = 200, w = 0.3, df = 1)
power_chi_square <- function(n, w, df = 1, alpha = 0.05) {
  ncp <- n * w^2
  chi_crit <- qchisq(1 - alpha, df = df)
  pchisq(chi_crit, df = df, ncp = ncp, lower.tail = FALSE)
}

#' Compute required sample size for a chi-square test
#'
#' @param w Cohen's w effect size.
#' @param df Degrees of freedom.
#' @param power Desired power.
#' @param alpha Significance level.
#' @return Named list with \code{n} and \code{achieved_power}.
#' @export
#' @examples
#' sample_size_chi_square(w = 0.3, df = 1, power = 0.80)
sample_size_chi_square <- function(w, df = 1, power = 0.80, alpha = 0.05) {
  lo <- 2L
  hi <- 1000000L
  if (power_chi_square(lo, w, df, alpha) >= power) {
    return(list(n = lo, achieved_power = power_chi_square(lo, w, df, alpha)))
  }
  while (hi - lo > 1L) {
    mid <- (lo + hi) %/% 2L
    pw <- power_chi_square(mid, w, df, alpha)
    if (pw >= power) hi <- mid else lo <- mid
  }
  list(n = hi, achieved_power = power_chi_square(hi, w, df, alpha))
}
