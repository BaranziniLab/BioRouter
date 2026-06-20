
# ---------------------------------------------------------------------------
# glm_fit.R — top-level my_glm() interface
# ---------------------------------------------------------------------------

my_glm = function(formula, data, family = "gaussian", maxit = 25, tol = 1e-8,
                  mustart = NULL, offset = NULL) {

  # build design matrix
  mm = build_model_matrix(formula, data)
  y  = mm$y
  X  = mm$X

  # resolve family
  family = resolve_family(family)

  # run IRLS
  fit = irls_fit(X, y, family, mustart = mustart, maxit = maxit, tol = tol,
                 intercept = TRUE, offset = offset)

  fit$formula  = formula
  fit$terms    = mm$terms
  fit$call     = match.call()

  structure(fit, class = "myglm")
}
