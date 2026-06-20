
# ---------------------------------------------------------------------------
# family.R — GLM family objects: gaussian, binomial, poisson
# Each family provides: link, linkinv, variance, dev.resids, mu.eta,
#   initialize, valideta
#
# Named gauss_family / binom_family / pois_family to avoid shadowing
# stats::gaussian etc.  my_glm() accepts character strings and resolves.
# ---------------------------------------------------------------------------

# Link function factory -----------------------------------------------------

make_link = function(link) {
  if (is.function(link)) return(link)

  switch(link,
    identity = list(
      linkfun  = function(mu) mu,
      inverse  = function(eta) eta,
      mu_eta   = function(eta) rep(1, length(eta))
    ),
    log = list(
      linkfun  = function(mu) log(pmax(mu, 1e-10)),
      inverse  = function(eta) exp(eta),
      mu_eta   = function(eta) exp(eta)
    ),
    logit = list(
      linkfun  = function(mu) log(pmax(mu, 1e-10) / pmax(1 - mu, 1e-10)),
      inverse  = function(eta) {
        p = exp(eta) / (1 + exp(eta))
        pmin(pmax(p, 1e-10), 1 - 1e-10)
      },
      mu_eta   = function(eta) {
        p = exp(eta) / (1 + exp(eta))
        pmax(p * (1 - p), 1e-10)
      }
    ),
    probit = list(
      linkfun  = function(mu) qnorm(pmin(pmax(mu, 1e-10), 1 - 1e-10)),
      inverse  = function(eta) pnorm(eta),
      mu_eta   = function(eta) dnorm(eta)
    ),
    cloglog = list(
      linkfun  = function(mu) log(-log(pmax(1 - mu, 1e-10))),
      inverse  = function(eta) 1 - exp(-exp(eta)),
      mu_eta   = function(eta) exp(eta) * exp(-exp(eta))
    ),
    sqrt = list(
      linkfun  = function(mu) sqrt(pmax(mu, 1e-10)),
      inverse  = function(eta) eta^2,
      mu_eta   = function(eta) 2 * eta
    ),
    inverse = list(
      linkfun  = function(mu) 1 / pmax(abs(mu), 1e-10),
      inverse  = function(eta) 1 / pmax(abs(eta), 1e-10),
      mu_eta   = function(eta) -1 / eta^2
    ),
    stop(sprintf("Unknown link function: '%s'", link))
  )
}

# gaussian family -----------------------------------------------------------

gauss_family = function(link = "identity") {
  lfun  = make_link(link)
  variance = function(mu) rep(1, length(mu))

  dev.resids = function(y, mu, wt) wt * (y - mu)^2

  aic = function(y, n, mu, wt, dev) {
    n * (log(dev / n * 2 * pi) + 1) + 2
  }

  initialize = function(y, nobs, mustart = NULL) {
    if (is.null(mustart)) mustart = y
    list(y = y, mustart = mustart, w = rep(1, nobs))
  }

  structure(
    list(family = "gaussian", link = link, linkfun = lfun$linkfun,
         linkinv = lfun$inverse, variance = variance,
         dev.resids = dev.resids, aic = aic, mu.eta = lfun$mu_eta,
         valideta = function(eta) rep(TRUE, length(eta)),
         initialize = initialize),
    class = "myglm_family"
  )
}

# binomial family -----------------------------------------------------------

binom_family = function(link = "logit") {
  lfun  = make_link(link)
  variance = function(mu) mu * (1 - mu)

  dev.resids = function(y, mu, wt) {
    m = 2 * wt
    a = y * log(pmax(y, 1e-10) / pmax(mu, 1e-10))
    b = (1 - y) * log(pmax(1 - y, 1e-10) / pmax(1 - mu, 1e-10))
    m * (a + b)
  }

  aic = function(y, n, mu, wt, dev) {
    ll = sum(wt * (y * log(pmax(mu, 1e-10)) + (1 - y) * log(pmax(1 - mu, 1e-10))))
    -2 * ll + 2
  }

  initialize = function(y, nobs, mustart = NULL) {
    if (is.null(mustart)) {
      mustart = pmax(pmin(y, 1 - 1e-5), 1e-5)
    }
    list(y = y, mustart = mustart, w = rep(1, nobs))
  }

  structure(
    list(family = "binomial", link = link, linkfun = lfun$linkfun,
         linkinv = lfun$inverse, variance = variance,
         dev.resids = dev.resids, aic = aic, mu.eta = lfun$mu_eta,
         valideta = function(eta) rep(TRUE, length(eta)),
         initialize = initialize),
    class = "myglm_family"
  )
}

# poisson family ------------------------------------------------------------

pois_family = function(link = "log") {
  lfun  = make_link(link)
  variance = function(mu) mu

  dev.resids = function(y, mu, wt) {
    term1 = y * log(pmax(y, 1e-10) / pmax(mu, 1e-10))
    term2 = (y - mu)
    2 * wt * (term1 - term2)
  }

  aic = function(y, n, mu, wt, dev) {
    2 * sum(wt * (mu - y * log(pmax(mu, 1e-10)))) + 2
  }

  initialize = function(y, nobs, mustart = NULL) {
    if (is.null(mustart)) mustart = y + 0.1
    list(y = y, mustart = mustart, w = rep(1, nobs))
  }

  structure(
    list(family = "poisson", link = link, linkfun = lfun$linkfun,
         linkinv = lfun$inverse, variance = variance,
         dev.resids = dev.resids, aic = aic, mu.eta = lfun$mu_eta,
         valideta = function(eta) rep(TRUE, length(eta)),
         initialize = initialize),
    class = "myglm_family"
  )
}

# Resolve a family name string to a family object ---------------------------

resolve_family = function(family) {
  if (inherits(family, "myglm_family")) return(family)
  if (is.character(family)) {
    fam_name = tolower(family[1])
    link = if (length(family) > 1) family[2] else NULL
    switch(fam_name,
      gaussian = if (!is.null(link)) gauss_family(link) else gauss_family(),
      binomial = if (!is.null(link)) binom_family(link) else binom_family(),
      poisson  = if (!is.null(link)) pois_family(link) else pois_family(),
      stop(sprintf("Unknown family: '%s'", family))
    )
  } else if (is.function(family)) {
    family()
  } else {
    stop("family must be a character string, function, or myglm_family object")
  }
}
