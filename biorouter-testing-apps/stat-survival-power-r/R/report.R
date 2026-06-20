#' Power-Analysis Summary Report
#'
#' Prints a formatted summary of a power analysis.
#'
#' @name report
NULL

# ---------------------------------------------------------------------------
# Exported functions
# ---------------------------------------------------------------------------

#' Print a power-analysis summary
#'
#' Evaluates the given power function with the provided arguments and
#' prints a clear, formatted summary.
#'
#' @param power_func A power function (e.g. \code{power_t_test}).
#' @param ... Arguments to \code{power_func}.
#' @param test_name Character label for the test (auto-detected if NULL).
#' @param n Optional pre-computed sample size (per group for two-sample).
#' @param power Optional pre-computed power.
#' @param d Optional effect size.
#' @return Invisibly returns the computed power.
#' @export
#' @examples
#' print_power_report(power_t_test, n = 30, d = 0.5, type = "two.sample")
print_power_report <- function(power_func, ..., test_name = NULL,
                               n = NULL, power = NULL, d = NULL) {
  # Compute if not provided
  args <- list(...)
  if (is.null(power)) {
    power <- tryCatch(do.call(power_func, args), error = function(e) NA)
  }
  if (is.null(n) && !is.null(args$n)) n <- args$n
  if (is.null(d) && !is.null(args$d)) d <- args$d

  # Detect test type
  if (is.null(test_name)) {
    test_name <- deparse(substitute(power_func))
  }

  # Header
  cat("\n")
  cat("==================================================\n")
  cat("  Power Analysis Report\n")
  cat("==================================================\n")
  cat(sprintf("  Test:        %s\n", test_name))
  cat(sprintf("  Parameters:  %s\n", paste(
    paste(names(args), "=", sapply(args, function(a) {
      if (is.numeric(a)) sprintf("%.4g", a) else as.character(a)
    }), sep = " "), collapse = ", "
  )))
  cat("--------------------------------------------------\n")

  if (!is.na(power)) {
    cat(sprintf("  Power:       %.4f (%.1f%%)\n", power, power * 100))
  } else {
    cat("  Power:       (could not compute)\n")
  }

  # Power quality assessment
  if (!is.na(power)) {
    if (power >= 0.80 && power < 0.90) {
      cat("  Assessment:  Adequate (80-90%)\n")
    } else if (power >= 0.90) {
      cat("  Assessment:  Excellent (>90%)\n")
    } else if (power >= 0.50) {
      cat("  Assessment:  Below conventional threshold (<80%)\n")
    } else {
      cat("  Assessment:  Very low — study likely underpowered\n")
    }
  }

  cat("==================================================\n\n")
  invisible(power)
}
