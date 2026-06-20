
# ---------------------------------------------------------------------------
# diagnostics.R — residual types and leverage
# ---------------------------------------------------------------------------

my_residuals = function(object, type = "deviance") {
  switch(type,
    working    = object$working.residuals,
    pearson    = object$pearson.residuals,
    deviance   = object$deviance.residuals,
    response   = object$y - object$mu,
    stop(sprintf("Unknown residual type: '%s'", type))
  )
}

my_hatvalues = function(object) object$hatvalues
