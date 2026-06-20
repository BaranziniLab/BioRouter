
# ---------------------------------------------------------------------------
# irls.R — Iteratively Reweighted Least Squares engine for GLMs
# ---------------------------------------------------------------------------

irls_fit = function(X, y, family, mustart = NULL, maxit = 25, tol = 1e-8,
                    intercept = TRUE, offset = NULL) {

  n = nrow(X)
  p = ncol(X)
  wt = rep(1, n)

  # initialise mu from family
  init = family$initialize(y, n, mustart)
  mu   = init$mustart
  wt   = init$w

  mu = clamp_mu(mu, family)
  eta = family$linkfun(mu)
  eta = clamp_eta(eta, family)
  if (!is.null(offset)) eta = eta + offset

  # initial beta via WLS on first working response
  devold = sum(family$dev.resids(y, mu, wt))
  if (!is.finite(devold)) devold = 1e20

  beta = rep(0, p)

  for (i in seq_len(maxit)) {
    mu_eta_val = family$mu.eta(eta)
    mu_eta_val = pmax(mu_eta_val, 1e-15)

    Vmu = family$variance(mu)
    Vmu = pmax(Vmu, 1e-15)

    W = as.numeric(wt * mu_eta_val^2 / Vmu)
    W = pmax(W, 1e-15)
    W = pmin(W, 1e10)

    z = (eta - if (!is.null(offset)) offset else 0) + (y - mu) / mu_eta_val
    z[!is.finite(z)] = 0

    Xw = X * sqrt(W)
    zw = z * sqrt(W)
    Xw[!is.finite(Xw)] = 0
    zw[!is.finite(zw)] = 0

    qr_obj = qr(Xw)
    beta_new = qr.coef(qr_obj, zw)
    beta_new[is.na(beta_new)] = 0

    # step-halving: shrink toward new beta until deviance improves
    step = 1.0
    for (s in 1:20) {
      beta_try = (1 - step) * beta + step * beta_new
      eta_try = drop(X %*% beta_try)
      if (!is.null(offset)) eta_try = eta_try + offset
      eta_try = clamp_eta(eta_try, family)
      mu_try  = family$linkinv(eta_try)
      mu_try  = clamp_mu(mu_try, family)
      dev_try = sum(family$dev.resids(y, mu_try, wt))
      if (!is.finite(dev_try)) dev_try = 1e20
      if (dev_try <= devold + 1e-8) break
      step = step / 2
    }

    beta = beta_try
    eta  = eta_try
    mu   = mu_try
    dev  = dev_try

    if (abs(dev - devold) / (abs(dev) + 1e-10) < tol) {
      devold = dev
      break
    }
    devold = dev
  }

  # --- final Fisher information and SEs ---
  mu_eta_val = family$mu.eta(eta)
  mu_eta_val = pmax(mu_eta_val, 1e-15)
  Vmu = family$variance(mu)
  Vmu = pmax(Vmu, 1e-15)

  W = as.numeric(wt * mu_eta_val^2 / Vmu)
  W = pmax(W, 1e-15)
  W = pmin(W, 1e10)

  Xw = X * sqrt(W)
  Xw[!is.finite(Xw)] = 0

  XtWX = crossprod(Xw)
  V = tryCatch(
    solve(XtWX),
    error = function(e) solve(XtWX + diag(1e-6, p))
  )

  # dispersion
  pearson_resid = (y - mu) / sqrt(Vmu)
  if (family$family == "gaussian") {
    dispersion = sum(wt * pearson_resid^2) / (n - p)
    if (!is.finite(dispersion) || dispersion < 1e-10) dispersion = 1
  } else {
    dispersion = 1
  }

  se = sqrt(abs(diag(V * dispersion)))

  H = Xw %*% V %*% t(Xw)
  hatvalues = pmin(pmax(diag(H), 0), 1 - 1e-10)

  zstat = beta / se

  # null deviance
  if (intercept) {
    if (family$family == "binomial") {
      p_bar = max(min(sum(y * wt) / sum(wt), 1 - 1e-10), 1e-10)
      null_mu = rep(p_bar, n)
    } else if (family$family == "poisson") {
      null_mu = rep(max(mean(y), 1e-10), n)
    } else {
      null_mu = rep(mean(y), n)
    }
    null_dev = sum(family$dev.resids(y, null_mu, wt))
  } else {
    null_dev = dev
  }

  rank = qr(XtWX)$rank
  df_resid = n - rank
  aic_val = family$aic(y, n, mu, wt, dev) + 2 * rank

  wresid = (y - mu) / mu_eta_val
  dev_resid = sign(y - mu) * sqrt(abs(family$dev.resids(y, mu, wt)))

  list(
    coefficients = beta,
    se           = se,
    zstat        = zstat,
    pvalue       = 2 * pnorm(-abs(zstat)),
    mu           = mu,
    eta          = eta,
    deviance     = dev,
    null.deviance = null_dev,
    dispersion   = dispersion,
    aic          = aic_val,
    df.residual  = df_resid,
    rank         = rank,
    df.null      = n - if (intercept) 1 else 0,
    iter         = min(i, maxit),
    converged    = (i < maxit || abs(dev - devold) / (abs(dev) + 1e-10) < tol),
    hatvalues    = hatvalues,
    working.residuals  = wresid,
    pearson.residuals  = pearson_resid,
    deviance.residuals = dev_resid,
    V   = V * dispersion,
    Vraw = V,
    X   = X,
    y   = y,
    wt  = wt,
    family   = family,
    formula  = NULL
  )
}

clamp_mu = function(mu, family) {
  if (family$family == "binomial") {
    mu = pmax(pmin(mu, 1 - 1e-7), 1e-7)
  } else if (family$family == "poisson") {
    mu = pmax(mu, 1e-7)
  }
  mu
}

clamp_eta = function(eta, family = NULL) {
  if (!is.null(family) && family$family == "binomial") {
    pmax(pmin(eta, 15), -15)
  } else if (!is.null(family) && family$family == "poisson") {
    pmax(pmin(eta, 20), -20)
  } else {
    pmax(pmin(eta, 30), -30)
  }
}
