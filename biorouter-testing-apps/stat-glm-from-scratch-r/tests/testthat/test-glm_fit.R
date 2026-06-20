test_that("gaussian IRLS recovers true coefficients", {
  dat = simulate_gaussian_data(n = 500, seed = 42)
  fit = my_glm(y ~ x1 + x2, data = dat, family = "gaussian")

  true_b = attr(dat, "true_beta")

  expect_equal(unname(fit$coefficients), true_b, tolerance = 0.15)

  ref = glm(y ~ x1 + x2, data = dat, family = stats::gaussian())
  expect_equal(unname(fit$coefficients), unname(coef(ref)), tolerance = 1e-6)
})

test_that("binomial IRLS recovers true coefficients", {
  dat = simulate_binomial_data(n = 500, seed = 123)
  fit = my_glm(y ~ x1 + x2, data = dat, family = "binomial")

  true_b = attr(dat, "true_beta")

  expect_equal(unname(fit$coefficients), true_b, tolerance = 0.3)

  ref = glm(y ~ x1 + x2, data = dat, family = stats::binomial())
  expect_equal(unname(fit$coefficients), unname(coef(ref)), tolerance = 1e-5)
})

test_that("poisson IRLS recovers true coefficients", {
  dat = simulate_poisson_data(n = 500, seed = 456)
  fit = my_glm(y ~ x1 + x2, data = dat, family = "poisson")

  true_b = attr(dat, "true_beta")

  expect_equal(unname(fit$coefficients), true_b, tolerance = 0.2)

  ref = glm(y ~ x1 + x2, data = dat, family = stats::poisson())
  expect_equal(unname(fit$coefficients), unname(coef(ref)), tolerance = 1e-5)
})

test_that("standard errors match base R glm()", {
  dat = simulate_gaussian_data(n = 300, seed = 99)
  fit = my_glm(y ~ x1 + x2, data = dat, family = "gaussian")
  ref = glm(y ~ x1 + x2, data = dat, family = stats::gaussian())

  expect_equal(unname(fit$se), unname(summary(ref)$coefficients[, "Std. Error"]),
               tolerance = 1e-4)
})

test_that("deviance and AIC match base R glm()", {
  dat = simulate_gaussian_data(n = 300, seed = 77)
  fit = my_glm(y ~ x1 + x2, data = dat, family = "gaussian")
  ref = glm(y ~ x1 + x2, data = dat, family = stats::gaussian())

  expect_equal(fit$deviance, deviance(ref), tolerance = 1e-6)
  expect_equal(fit$aic, AIC(ref), tolerance = 1e-4)
})

test_that("formula with factors works", {
  dat = simulate_factor_data(n = 300, seed = 789)
  fit = my_glm(y ~ group + x1, data = dat, family = "gaussian")
  ref = glm(y ~ group + x1, data = dat, family = stats::gaussian())

  expect_equal(unname(fit$coefficients), unname(coef(ref)), tolerance = 1e-6)
})

test_that("p-values are in [0,1]", {
  dat = simulate_gaussian_data(n = 100)
  fit = my_glm(y ~ x1 + x2, data = dat)
  expect_true(all(fit$pvalue >= 0 & fit$pvalue <= 1))
})

test_that("convergence is reported", {
  dat = simulate_gaussian_data(n = 100)
  fit = my_glm(y ~ x1 + x2, data = dat)
  expect_true(fit$converged)
  expect_true(fit$iter >= 1)
})

test_that("probit link works for binomial", {
  dat = simulate_binomial_data(n = 300, seed = 101)
  fit = my_glm(y ~ x1 + x2, data = dat, family = c("binomial", "probit"))
  ref = glm(y ~ x1 + x2, data = dat, family = stats::binomial(link = "probit"))

  expect_equal(unname(fit$coefficients), unname(coef(ref)), tolerance = 1e-4)
})
