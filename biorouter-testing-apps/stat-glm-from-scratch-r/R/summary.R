
# ---------------------------------------------------------------------------
# summary.R — print and summary methods for myglm objects
# ---------------------------------------------------------------------------

print.myglm = function(x, ...) {
  cat("\nCall:\n")
  print(x$call)
  cat("\nCoefficients:\n")
  tab = data.frame(
    Estimate = x$coefficients,
    Std.Err  = x$se,
    z.value  = x$zstat,
    Pr.z     = x$pvalue
  )
  rownames(tab) = names(x$coefficients)
  printCoefmat(tab, digits = 4, signif.legend = FALSE)
  cat(sprintf("\nResidual deviance:  %.4f  on %d degrees of freedom\n",
              x$deviance, x$df.residual))
  cat(sprintf("Null deviance:      %.4f  on %d degrees of freedom\n",
              x$null.deviance, x$df.null))
  cat(sprintf("AIC: %.4f\n", x$aic))
  cat(sprintf("Number of Fisher Scoring iterations: %d\n", x$iter))
  cat("\n")
  invisible(x)
}

summary.myglm = function(object, ...) {
  tab = data.frame(
    Estimate  = object$coefficients,
    Std.Err   = object$se,
    z.value   = object$zstat,
    Pr.z      = object$pvalue
  )
  rownames(tab) = names(object$coefficients)

  out = list(
    call          = object$call,
    coefficients  = tab,
    deviance      = object$deviance,
    df.residual   = object$df.residual,
    null.deviance = object$null.deviance,
    df.null       = object$df.null,
    aic           = object$aic,
    iter          = object$iter,
    family        = object$family$family,
    link          = object$family$link
  )
  class(out) = "myglm_summary"
  out
}

print.myglm_summary = function(x, digits = 4, ...) {
  cat("\nCall:\n")
  print(x$call)
  cat(sprintf("\nFamily: %s (link: %s)\n\n", x$family, x$link))
  cat("Coefficients:\n")
  printCoefmat(x$coefficients, digits = digits, signif.legend = TRUE)
  cat(sprintf("\nResidual deviance:  %.*f  on %d degrees of freedom\n",
              digits, x$deviance, x$df.residual))
  cat(sprintf("Null deviance:      %.*f  on %d degrees of freedom\n",
              digits, x$null.deviance, x$df.null))
  cat(sprintf("AIC: %.*f\n", digits, x$aic))
  cat(sprintf("Number of Fisher Scoring iterations: %d\n", x$iter))
  cat("\n")
  invisible(x)
}
