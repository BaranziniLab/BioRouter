test_that("deviance residuals have correct sign", {
  dat = simulate_gaussian_data(n = 100)
  fit = my_glm(y ~ x1 + x2, data = dat)
  dr = my_residuals(fit, "deviance")
  expect_equal(sign(dr), sign(dat$y - fit$mu))
})

test_that("pearson residuals are bounded reasonably", {
  dat = simulate_gaussian_data(n = 100)
  fit = my_glm(y ~ x1 + x2, data = dat)
  pr = my_residuals(fit, "pearson")
  expect_true(all(is.finite(pr)))
  expect_true(abs(mean(pr)) < 0.5)
})

test_that("hatvalues are between 0 and 1", {
  dat = simulate_gaussian_data(n = 100)
  fit = my_glm(y ~ x1 + x2, data = dat)
  hv = my_hatvalues(fit)
  expect_true(all(hv >= 0))
  expect_true(all(hv < 1))
})

test_that("sum of hatvalues equals rank (p)", {
  dat = simulate_gaussian_data(n = 100)
  fit = my_glm(y ~ x1 + x2, data = dat)
  hv = my_hatvalues(fit)
  p = length(fit$coefficients)
  expect_equal(sum(hv), p, tolerance = 1e-6)
})

test_that("working residuals match formula", {
  dat = simulate_gaussian_data(n = 100)
  fit = my_glm(y ~ x1 + x2, data = dat)
  wr = my_residuals(fit, "working")
  mu_eta_val = fit$family$mu.eta(fit$eta)
  expected = (dat$y - fit$mu) / mu_eta_val
  expect_equal(wr, expected, tolerance = 1e-10)
})

test_that("response residuals are y - mu", {
  dat = simulate_gaussian_data(n = 100)
  fit = my_glm(y ~ x1 + x2, data = dat)
  rr = my_residuals(fit, "response")
  expect_equal(rr, dat$y - fit$mu)
})
