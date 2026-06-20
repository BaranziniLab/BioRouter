#' Universal Power-Analysis Solver
#'
#' Given a power function and all parameters except one, solve for the
#' missing parameter using bisection search.
#'
#' @name solver
NULL

# ---------------------------------------------------------------------------
# Exported functions
# ---------------------------------------------------------------------------

#' Solve for any one power-analysis parameter
#'
#' Bisection solver that finds the value of a named parameter that
#' makes a power function return a target value.
#'
#' @param power_func A function whose first unnamed argument is the
#'   parameter to solve for (e.g. \code{power_t_test}).
#' @param target One of \code{"power"}, \code{"n"}, \code{"d"} (effect size),
#'   or \code{"alpha"}.
#' @param target_value The desired value for the target (default: power = 0.80
#'   for power target, etc.).
#' @param ... Additional named arguments passed to \code{power_func}.
#' @param lo Lower bound of search.
#' @param hi Upper bound of search.
#' @param tol Convergence tolerance.
#' @return Named list: \code{target}, \code{found_value}, \code{params} (all parameters).
#' @export
#' @examples
#' # Solve for n to achieve 80% power
#' solve_power(power_t_test, target = "n", target_value = 0.80,
#'             d = 0.5, type = "two.sample")
#'
#' # Solve for effect size
#' solve_power(power_t_test, target = "d", target_value = 0.80,
#'             n = 30, type = "two.sample")
solve_power <- function(power_func, target = c("power", "n", "d", "alpha"),
                        target_value = NULL,
                        lo = NULL, hi = NULL, tol = 1e-6,
                        ...) {
  target <- match.arg(target)

  # Defaults for search bounds
  if (is.null(lo)) {
    lo <- switch(target,
      power  = 0.001,
      n      = 2,
      d      = 0.001,
      alpha  = 1e-6
    )
  }
  if (is.null(hi)) {
    hi <- switch(target,
      power  = 0.9999,
      n      = 1e6,
      d      = 5.0,
      alpha  = 0.5
    )
  }
  if (is.null(target_value)) {
    target_value <- switch(target,
      power = 0.80,
      n     = stop("target_value required for target='n'"),
      d     = stop("target_value required for target='d'"),
      alpha = 0.05
    )
  }

  # Build a wrapper that varies only the target parameter
  .wrapper <- function(x) {
    args <- list(...)
    args[[target]] <- x
    do.call(power_func, args)
  }

  # Check endpoints
  f_lo <- .wrapper(lo)
  f_hi <- .wrapper(hi)

  if (is.na(f_lo) || is.na(f_hi)) {
    stop("Power function returned NA at search boundaries.")
  }

  # For "n" and "d", power is monotonically increasing
  # For "alpha", power is monotonically increasing
  # For "power", the function evaluates power at given n — inverse: find n for target power
  if (target == "power") {
    # Actually: find parameter such that power_func(params) == target_value
    # We search over the parameter that makes the function output match
    # For power target, we typically solve for n (this is handled by n target)
    # Here: solve for parameter that gives target power
    # The wrapper varies the named target parameter
  }

  # Bisection: find x such that .wrapper(x) = target_value
  for (i in 1:200) {
    mid <- (lo + hi) / 2
    f_mid <- .wrapper(mid)

    if (abs(f_mid - target_value) < tol) {
      found <- mid
      break
    }

    # Determine direction: for most params, higher x -> higher power
    if (f_mid < target_value) {
      lo <- mid
    } else {
      hi <- mid
    }
    found <- mid
  }

  # Final parameters
  params <- list(...)
  params[[target]] <- found

  list(
    target = target,
    found_value = found,
    achieved_value = .wrapper(found),
    params = params
  )
}
