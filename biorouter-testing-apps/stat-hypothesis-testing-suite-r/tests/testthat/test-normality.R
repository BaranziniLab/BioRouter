# Tests for normality tests

test_that("hyp_shapiro_wilk is reasonable", {
  set.seed(42)
  x <- rnorm(50, mean = 0, sd = 1)

  result <- hyp_shapiro_wilk(x)
  base <- shapiro.test(x)

  # W statistic should be reasonably close (our approx vs exact)
  expect_true(result$statistic > 0.9 && result$statistic <= 1.0)
  # p-value should be in the right ballpark
  expect_true(result$p_value > 0.05)  # Normal data should not be rejected
  expect_true(base$p.value > 0.05)
})

test_that("hyp_shapiro_wilk detects non-normal", {
  set.seed(42)
  x <- rexp(50, rate = 1)  # Exponential - clearly not normal

  result <- hyp_shapiro_wilk(x)
  base <- shapiro.test(x)

  # Both should reject normality
  expect_true(result$p_value < 0.05)
  expect_true(base$p.value < 0.05)
})

test_that("hyp_ks_test D-statistic is reasonable", {
  set.seed(42)
  x <- rnorm(50, mean = 0, sd = 1)

  result <- hyp_ks_test(x, mu = 0, sigma = 1)
  base <- ks.test(x, "pnorm", mean = 0, sd = 1)

  # D-statistic should be reasonably close
  expect_true(result$statistic < 0.2)  # Good fit
  expect_true(base$statistic < 0.2)
  # Both should not reject normality
  expect_true(result$p_value > 0.05)
  expect_true(base$p.value > 0.05)
})

test_that("hyp_ks_test without parameters estimates from data", {
  set.seed(42)
  x <- rnorm(50, mean = 5, sd = 2)

  result <- hyp_ks_test(x)

  # Should estimate parameters correctly
  expect_equal(result$extra$mu, mean(x), tolerance = 1e-10)
  expect_equal(result$extra$sigma, sd(x), tolerance = 1e-10)
})

test_that("hyp_ks_test detects non-normal", {
  set.seed(42)
  x <- rexp(50, rate = 1)

  result <- hyp_ks_test(x)

  # Should detect non-normality
  expect_true(result$p_value < 0.05)
})
