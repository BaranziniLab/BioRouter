test_that("link functions round-trip correctly", {
  links = c("identity", "log", "logit", "probit", "cloglog")

  for (lk in links) {
    lfun = make_link(lk)
    mu_vals = switch(lk,
      logit   = c(0.01, 0.1, 0.3, 0.5, 0.7, 0.9, 0.99),
      probit  = c(0.01, 0.1, 0.3, 0.5, 0.7, 0.9, 0.99),
      cloglog = c(0.01, 0.1, 0.3, 0.5, 0.7, 0.9, 0.99),
      c(0.01, 0.5, 1.0, 2.0, 5.0, 10.0)
    )

    eta = lfun$linkfun(mu_vals)
    mu_back = lfun$inverse(eta)
    expect_equal(mu_back, mu_vals, tolerance = 1e-6,
                 info = paste("Round-trip failed for link:", lk))
  }
})

test_that("mu_eta matches derivative of inverse link", {
  links = c("identity", "log", "logit")
  for (lk in links) {
    lfun = make_link(lk)
    eta = seq(-2, 2, length.out = 20)
    eps = 1e-6
    num_deriv = (lfun$inverse(eta + eps) - lfun$inverse(eta - eps)) / (2 * eps)
    analytic  = lfun$mu_eta(eta)
    expect_equal(analytic, num_deriv, tolerance = 1e-4,
                 info = paste("mu_eta mismatch for link:", lk))
  }
})

test_that("gaussian variance returns 1 for all mu", {
  fam = gauss_family()
  expect_equal(fam$variance(c(0, 1, 5, 100)), rep(1, 4))
})

test_that("binomial variance = mu*(1-mu)", {
  fam = binom_family()
  mu = c(0.1, 0.3, 0.5, 0.7, 0.9)
  expect_equal(fam$variance(mu), mu * (1 - mu))
})

test_that("poisson variance = mu", {
  fam = pois_family()
  mu = c(0.5, 1, 2, 10)
  expect_equal(fam$variance(mu), mu)
})

test_that("gaussian dev.resids match (y-mu)^2", {
  fam = gauss_family()
  y  = c(1, 2, 3)
  mu = c(1.1, 1.8, 3.2)
  expect_equal(fam$dev.resids(y, mu, 1), (y - mu)^2)
})
