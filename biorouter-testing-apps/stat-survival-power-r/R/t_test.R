#' Power and Sample Size for t-Tests
#'
#' Compute power or required sample size for one-sample, two-sample
#' (independent), or paired t-tests using the non-central t distribution.
#'
#' @name t_test_power
NULL

# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------

# Degrees of freedom for each test type
.dof <- function(n, type = c("one.sample", "two.sample", "paired")) {
  type <- match.arg(type)
  switch(type,
    one.sample  = n - 1,
    two.sample  = 2 * n - 2,     # n per group
    paired      = n - 1
  )
}

# Non-centrality parameter
.ncp_t <- function(n, d, type = c("one.sample", "two.sample", "paired")) {
  type <- match.arg(type)
  switch(type,
    one.sample  = d * sqrt(n),
    two.sample  = d * sqrt(n / 2),   # n per group
    paired      = d * sqrt(n)
  )
}

# ---------------------------------------------------------------------------
# Power functions
# ---------------------------------------------------------------------------

#' Compute power for a t-test
#'
#' Uses the non-central t distribution. For two-sample tests, \code{n}
#' is the number of subjects *per group*.
#'
#' @param n Sample size (per group for two.sample; total for one.sample/paired).
#' @param d Cohen's d effect size.
#' @param alpha Significance level (default 0.05).
#' @param type One of \code{"one.sample"}, \code{"two.sample"}, \code{"paired"}.
#' @return Power (probability of rejecting H0).
#' @export
#' @examples
#' power_t_test(n = 30, d = 0.5)
#' power_t_test(n = 30, d = 0.5, type = "paired")
power_t_test <- function(n, d, alpha = 0.05, type = c("one.sample", "two.sample", "paired")) {
  type <- match.arg(type)
  df <- .dof(n, type)
  ncp <- .ncp_t(n, d, type)
  t_crit <- qt(1 - alpha / 2, df = df)
  power <- pt(t_crit, df = df, ncp = ncp, lower.tail = FALSE) +
           pt(-t_crit, df = df, ncp = ncp, lower.tail = TRUE)
  power
}

#' Compute required sample size for a t-test
#'
#' @param d Cohen's d effect size.
#' @param power Desired power.
#' @param alpha Significance level.
#' @param type One of \code{"one.sample"}, \code{"two.sample"}, \code{"paired"}.
#' @return Named list with \code{n} (per group for two.sample) and \code{achieved_power}.
#' @export
#' @examples
#' sample_size_t_test(d = 0.5, power = 0.80)
sample_size_t_test <- function(d, power = 0.80, alpha = 0.05,
                               type = c("one.sample", "two.sample", "paired")) {
  type <- match.arg(type)
  # Binary search for n
  lo <- 2L
  hi <- 100000L
  # First check if lo is enough
  if (power_t_test(lo, d, alpha, type) >= power) {
    return(list(n = lo, achieved_power = power_t_test(lo, d, alpha, type)))
  }
  while (hi - lo > 1L) {
    mid <- (lo + hi) %/% 2L
    pw <- power_t_test(mid, d, alpha, type)
    if (pw >= power) {
      hi <- mid
    } else {
      lo <- mid
    }
  }
  list(n = hi, achieved_power = power_t_test(hi, d, alpha, type))
}
