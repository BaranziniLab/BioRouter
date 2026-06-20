#' Package initialization
#'
#' @param libname Library name
#' @param pkgname Package name
#' @keywords internal
.onAttach <- function(libname, pkgname) {
  packageStartupMessage(
    "hypTestSuite v0.1.0 loaded.\n",
    "  Implements: t-tests, ANOVA, non-parametric, chi-square,\n",
    "  normality, corrections, power analysis, and reporting."
  )
}
