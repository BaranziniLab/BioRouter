#' Effect-Size Conversion Helpers
#'
#' Functions to convert between common effect-size measures:
#' Cohen's d (mean difference / SD), Cohen's f (ANOVA), Cohen's h
#' (arcsine difference for proportions), and Cohen's w (chi-square).
#'
#' @name effect_sizes
#' @aliases cohen_d_to_f cohen_d_to_h cohen_h_to_d cohen_h_to_w
#'          cohen_w_to_h effect_size_from_cohens_d effect_size_from_cohens_f
NULL

# ---------------------------------------------------------------------------
# Cohen's d <-> f
# ---------------------------------------------------------------------------

#' Convert Cohen's d to Cohen's f
#'
#' @param d Cohen's d (mean difference pooled SD).
#' @return Cohen's f.
#' @export
#' @examples
#' cohen_d_to_f(0.5)
cohen_d_to_f <- function(d) {
  d / 2.0
}

#' Convert Cohen's f to Cohen's d
#'
#' @param f Cohen's f.
#' @return Cohen's d.
#' @export
#' @examples
#' effect_size_from_cohens_f(0.25)
effect_size_from_cohens_f <- function(f) {
  2.0 * f
}

#' Convert Cohen's d to Cohen's h (proportions)
#'
#' Maps a continuous Cohen's d to the arcsine-scale Cohen's h via
#' an approximation: h ≈ 2 * arcsin(sqrt(p1)) - 2 * arcsin(sqrt(p2))
#' where p1 and p2 are derived from d via the logistic approximation.
#'
#' @param d Cohen's d.
#' @return Cohen's h (arcsine difference).
#' @export
#' @examples
#' cohen_d_to_h(0.5)
cohen_d_to_h <- function(d) {
  # Approximate conversion via logistic link:
  # d = ln(p1/(1-p1)) - ln(p2/(1-p2)) where p2 = logistic(-d/2), p1 = logistic(d/2)
  p1 <- 1 / (1 + exp(-d / 2))
  p2 <- 1 / (1 + exp( d / 2))
  2 * asin(sqrt(p1)) - 2 * asin(sqrt(p2))
}

# ---------------------------------------------------------------------------
# Cohen's h <-> Cohen's d
# ---------------------------------------------------------------------------

#' Convert Cohen's h to Cohen's d
#'
#' Inverse of [cohen_d_to_h()].
#'
#' @param h Cohen's h (arcsine difference).
#' @return Cohen's d.
#' @export
#' @examples
#' cohen_h_to_d(0.5)
cohen_h_to_d <- function(h) {
  # Numerical inverse: find d such that cohen_d_to_d(h) == h
  # Use bisection
  lo <- 0
  hi <- 10
  for (i in 1:100) {
    mid <- (lo + hi) / 2
    if (abs(cohen_d_to_h(mid) - h) < 1e-10) return(mid)
    if (cohen_d_to_h(mid) < h) lo <- mid else hi <- mid
  }
  (lo + hi) / 2
}

#' Convert Cohen's h to Cohen's w (chi-square)
#'
#' For a 2x2 table, w = h / sqrt(2) for equal marginal proportions.
#' More generally, w ≈ h / 2 * sqrt(1/(p1*(1-p1)) + 1/(p2*(1-p2))),
#' but the simple approximation is used here.
#'
#' @param h Cohen's h.
#' @param p Average proportion (default 0.5 for equal marginals).
#' @return Cohen's w.
#' @export
#' @examples
#' cohen_h_to_w(0.5)
cohen_h_to_w <- function(h, p = 0.5) {
  # For a 2x2 chi-square: w = h / sqrt(2) when p1 = p2 = 0.5
  h / sqrt(2)
}

#' Convert Cohen's w to Cohen's h
#'
#' @param w Cohen's w.
#' @return Cohen's h.
#' @export
#' @examples
#' cohen_w_to_h(0.35)
cohen_w_to_h <- function(w) {
  w * sqrt(2)
}

# ---------------------------------------------------------------------------
# Effect-size from Cohen's d (general)
# ---------------------------------------------------------------------------

#' Compute partial eta-squared or other effect-size measures from Cohen's d
#'
#' Returns a list with eta-squared, omega-squared, and f from d.
#'
#' @param d Cohen's d.
#' @return Named list: eta_sq, omega_sq, f.
#' @export
#' @examples
#' effect_size_from_cohens_d(0.5)
effect_size_from_cohens_d <- function(d) {
  f_val <- cohen_d_to_f(d)
  eta_sq <- f_val^2 / (1 + f_val^2)
  omega_sq <- (f_val^2 - d / (d + 2))^2 / (1 + f_val^2)  # approximate
  list(
    eta_sq = eta_sq,
    omega_sq = max(0, omega_sq),
    f = f_val
  )
}
