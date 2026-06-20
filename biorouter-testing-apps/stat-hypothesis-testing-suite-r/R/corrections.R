#' Multiple Comparison Corrections
#'
#' Implements Bonferroni, Holm, and Benjamini-Hochberg FDR corrections.

#' Bonferroni correction
#'
#' @param p_values Numeric vector of p-values
#' @param alpha Numeric: family-wise significance level (default 0.05)
#' @return A data.frame with original p-values, adjusted p-values, and decisions
#' @export
corr_bonferroni <- function(p_values, alpha = 0.05) {
  m <- length(p_values)
  adjusted <- p_values * m
  adjusted <- pmin(adjusted, 1)  # Cap at 1

  result <- data.frame(
    p_raw = p_values,
    p_adjusted = adjusted,
    significant = adjusted < alpha,
    stringsAsFactors = FALSE
  )

  return(result)
}

#' Holm (step-down) correction
#'
#' @param p_values Numeric vector of p-values
#' @param alpha Numeric: family-wise significance level (default 0.05)
#' @return A data.frame with original p-values, adjusted p-values, and decisions
#' @export
corr_holm <- function(p_values, alpha = 0.05) {
  m <- length(p_values)
  order_idx <- order(p_values)
  sorted <- p_values[order_idx]

  adjusted_sorted <- numeric(m)
  for (i in 1:m) {
    adjusted_sorted[i] <- sorted[i] * (m - i + 1)
  }

  # Enforce monotonicity (step-down)
  for (i in (m - 1):1) {
    adjusted_sorted[i] <- min(adjusted_sorted[i], adjusted_sorted[i + 1])
  }

  adjusted_sorted <- pmin(adjusted_sorted, 1)

  # Map back to original order
  adjusted <- numeric(m)
  adjusted[order_idx] <- adjusted_sorted

  result <- data.frame(
    p_raw = p_values,
    p_adjusted = adjusted,
    significant = adjusted < alpha,
    stringsAsFactors = FALSE
  )

  return(result)
}

#' Benjamini-Hochberg FDR correction
#'
#' @param p_values Numeric vector of p-values
#' @param alpha Numeric: false discovery rate level (default 0.05)
#' @return A data.frame with original p-values, adjusted p-values, and decisions
#' @export
corr_bh_fdr <- function(p_values, alpha = 0.05) {
  m <- length(p_values)
  order_idx <- order(p_values)
  sorted <- p_values[order_idx]

  adjusted_sorted <- numeric(m)
  for (i in 1:m) {
    adjusted_sorted[i] <- sorted[i] * m / i
  }

  # Enforce monotonicity (step-up)
  for (i in (m - 1):1) {
    adjusted_sorted[i] <- min(adjusted_sorted[i], adjusted_sorted[i + 1])
  }

  adjusted_sorted <- pmin(adjusted_sorted, 1)

  # Map back to original order
  adjusted <- numeric(m)
  adjusted[order_idx] <- adjusted_sorted

  result <- data.frame(
    p_raw = p_values,
    p_adjusted = adjusted,
    significant = adjusted < alpha,
    stringsAsFactors = FALSE
  )

  return(result)
}
