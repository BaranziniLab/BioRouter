# Tests for utility functions

test_that("tidy_result creates valid result", {
  res <- tidy_result(
    test_name = "Test",
    statistic = 2.5,
    df = 10,
    p_value = 0.03,
    effect_size = 0.5,
    effect_name = "d"
  )

  expect_s3_class(res, "hyp_result")
  expect_equal(res$test_name, "Test")
  expect_equal(res$statistic, 2.5)
  expect_equal(res$p_value, 0.03)
  expect_true(res$significant)
})

test_that("tidy_result clamps p-value to [0, 1]", {
  res <- tidy_result("Test", 1, 10, p_value = 1.5)
  expect_equal(res$p_value, 1)

  res <- tidy_result("Test", 1, 10, p_value = -0.1)
  expect_equal(res$p_value, 0)
})

test_that("effects_cohens_d matches manual calculation", {
  set.seed(42)
  x <- rnorm(30, mean = 1, sd = 1)
  y <- rnorm(30, mean = 0, sd = 1)

  # Manual calculation
  n1 <- length(x)
  n2 <- length(y)
  sp <- sqrt(((n1 - 1) * var(x) + (n2 - 1) * var(y)) / (n1 + n2 - 2))
  expected_d <- (mean(x) - mean(y)) / sp

  d <- effects_cohens_d(x, y)
  expect_equal(d, expected_d, tolerance = 1e-10)
})

test_that("effects_hedges_g is bias-corrected", {
  set.seed(123)
  x <- rnorm(20)
  y <- rnorm(20) + 0.5

  d <- effects_cohens_d(x, y)
  g <- effects_hedges_g(x, y)

  n <- length(x) + length(y)
  correction <- 1 - 3 / (4 * (n - 1) - 1)
  expect_equal(g, d * correction, tolerance = 1e-10)
})

test_that("effects_eta_squared is correct", {
  eta2 <- effects_eta_squared(50, 100)
  expect_equal(eta2, 0.5)
})

test_that("effects_epsilon_squared is correct", {
  eps2 <- effects_epsilon_squared(0.5, 2, 98)
  expected <- 0.5 - (2 * (1 - 0.5)) / (99 - 2)
  expect_equal(eps2, expected, tolerance = 1e-10)
})

test_that("ci_t_mean matches base R t.test CI", {
  set.seed(42)
  x <- rnorm(30, mean = 5, sd = 2)

  ci <- ci_t_mean(x, 0.95)
  base_ci <- t.test(x, conf.level = 0.95)$conf.int

  expect_equal(ci$lower, base_ci[1], tolerance = 1e-10)
  expect_equal(ci$upper, base_ci[2], tolerance = 1e-10)
})

test_that("ci_correlation is valid", {
  ci <- ci_correlation(0.5, 30, 0.95)
  expect_true(ci$lower < 0.5)
  expect_true(ci$upper > 0.5)
  expect_true(ci$lower > -1)
  expect_true(ci$upper < 1)
})

test_that("norm_cdf matches base R", {
  expect_equal(norm_cdf(0), 0.5)
  expect_equal(norm_cdf(1), pnorm(1), tolerance = 1e-6)
  expect_equal(norm_cdf(-1), pnorm(-1), tolerance = 1e-6)
  expect_equal(norm_cdf(2), pnorm(2), tolerance = 1e-6)
})

test_that("t_cdf matches base R", {
  expect_equal(t_cdf(0, 10), 0.5)
  expect_equal(t_cdf(1, 10), pt(1, 10), tolerance = 1e-6)
  expect_equal(t_cdf(-1, 10), pt(-1, 10), tolerance = 1e-6)
  expect_equal(t_cdf(2, 29), pt(2, 29), tolerance = 1e-6)
})

test_that("f_cdf matches base R", {
  expect_equal(f_cdf(0, 5, 10), 0)
  expect_equal(f_cdf(1, 5, 10), pf(1, 5, 10), tolerance = 1e-6)
  expect_equal(f_cdf(3, 10, 20), pf(3, 10, 20), tolerance = 1e-6)
})

test_that("chisq_cdf matches base R", {
  expect_equal(chisq_cdf(0, 5), 0)
  expect_equal(chisq_cdf(5, 5), pchisq(5, 5), tolerance = 1e-6)
  expect_equal(chisq_cdf(10, 5), pchisq(10, 5), tolerance = 1e-6)
})
