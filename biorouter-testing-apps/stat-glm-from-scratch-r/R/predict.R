
# ---------------------------------------------------------------------------
# predict.R — predictions on link and response scale with CIs
# ---------------------------------------------------------------------------

predict.myglm = function(object, newdata = NULL, type = "link", se.fit = FALSE,
                          ci = FALSE, ci.level = 0.95, ...) {

  if (is.null(newdata)) {
    X = object$X
  } else {
    # remove response from terms for newdata
    tt = delete.response(object$terms)
    mf = stats::model.frame(tt, data = newdata, na.action = na.pass)
    X  = stats::model.matrix(tt, data = mf)
  }

  eta = drop(X %*% object$coefficients)
  fit_link = eta
  fit_resp = object$family$linkinv(eta)

  if (se.fit || ci) {
    V = object$V
    se_link = sqrt(pmax(rowSums((X %*% V) * X), 0))

    mu_eta_val = object$family$mu.eta(eta)
    se_resp = se_link * abs(mu_eta_val)
  }

  # decide what to return
  if (type == "link") {
    fit = fit_link
    se  = if (se.fit || ci) se_link else NULL
  } else if (type == "response") {
    fit = fit_resp
    se  = if (se.fit || ci) se_resp else NULL
  } else {
    stop(sprintf("Unknown type: '%s'", type))
  }

  if (!ci && !se.fit) return(fit)

  result = list(fit = fit)
  if (se.fit) result$se.fit = se

  if (ci) {
    zval = stats::qnorm((1 + ci.level) / 2)
    if (type == "link") {
      result$lwr = fit - zval * se_link
      result$upr = fit + zval * se_link
    } else {
      lwr_link = fit_link - zval * se_link
      upr_link = fit_link + zval * se_link
      result$lwr = object$family$linkinv(lwr_link)
      result$upr = object$family$linkinv(upr_link)
    }
  }

  result
}
