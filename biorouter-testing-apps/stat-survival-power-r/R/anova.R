#' Power and Sample Size for One-Way ANOVA
#'
#' Uses the non-central F distribution with Cohen's f as effect size.
#'
#' @name anova_power
NULL

# ---------------------------------------------------------------------------
# Internal
# ---------------------------------------------------------------------------

.ncp_anova <- function(k, n, f) {
  # k = number of groups, n = per group
  k * n * f^2
}

# ---------------------------------------------------------------------------
# Exported functions
# ---------------------------------------------------------------------------

#' Compute power for a one-way ANOVA
#'
#' @param n Sample size per group.
#' @param k Number of groups.
#' @param f Cohen's f effect size.
#' @param alpha Significance level (default 0.05).
#' @return Power.
#' @export
#' @examples
#' power_anova(n = 20, k = 3, f = 0.25)
power_anova <- function(n, k, f, alpha = 0.05) {
  df1 <- k - 1
  df2 <- k * (n - 1)
  ncp <- .ncp_anova(k, n, f)
  f_crit <- qf(1 - alpha, df1 = df1, df2 = df2)
  pf(f_crit, df1 = df1, df2 = df2, ncp = ncp, lower.tail = FALSE)
}

#' Compute required sample size per group for a one-way ANOVA
#'
#' @param k Number of groups.
#' @param f Cohen's f.
#' @param power Desired power.
#' @param alpha Significance level.
#' @return Named list with \code{n} (per group) and \code{achieved_power}.
#' @export
#' @examples
#' sample_size_anova(k = 3, f = 0.25, power = 0.80)
sample_size_anova <- function(k, f, power = 0.80, alpha = 0.05) {
  lo <- 2L
  hi <- 100000L
  if (power_anova(lo, k, f, alpha) >= power) {
    return(list(n = lo, achieved_power = power_anova(lo, k, f, alpha)))
  }
  while (hi - lo > 1L) {
    mid <- (lo + hi) %/% 2L
    pw <- power_anova(mid, k, f, alpha)
    if (pw >= power) hi <- mid else lo <- mid
  }
  list(n = hi, achieved_power = power_anova(hi, k, f, alpha))
}
