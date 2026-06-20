library(testthat)
library(statSurvivalPower)

test_that("power_t_test returns a scalar between 0 and 1", {
  pw <- power_t_test(n = 30, d = 0.5, type = "two.sample")
  expect_length(pw, 1)
  expect_true(pw > 0 && pw < 1)
})

test_that("power_t_test two-sample matches pwr::pwr.t.test", {
  skip_if_not_installed("pwr")
  for (d_val in c(0.2, 0.5, 0.8)) {
    for (n_val in c(20, 50, 100)) {
      ours <- power_t_test(n = n_val, d = d_val, type = "two.sample")
      theirs <- pwr::pwr.t.test(n = n_val, d = d_val, sig.level = 0.05,
                                 type = "two.sample", alternative = "two.sided")$power
      expect_equal(ours, theirs, tolerance = 0.01,
                   info = paste("d =", d_val, "n =", n_val))
    }
  }
})

test_that("power_t_test one-sample matches pwr::pwr.t.test", {
  skip_if_not_installed("pwr")
  for (d_val in c(0.3, 0.6)) {
    for (n_val in c(15, 40)) {
      ours <- power_t_test(n = n_val, d = d_val, type = "one.sample")
      theirs <- pwr::pwr.t.test(n = n_val, d = d_val, sig.level = 0.05,
                                 type = "one.sample", alternative = "two.sided")$power
      expect_equal(ours, theirs, tolerance = 0.01,
                   info = paste("d =", d_val, "n =", n_val))
    }
  }
})

test_that("power_t_test paired matches pwr::pwr.t.test", {
  skip_if_not_installed("pwr")
  for (d_val in c(0.4, 0.7)) {
    ours <- power_t_test(n = 25, d = d_val, type = "paired")
    theirs <- pwr::pwr.t.test(n = 25, d = d_val, sig.level = 0.05,
                               type = "paired", alternative = "two.sided")$power
    expect_equal(ours, theirs, tolerance = 0.01,
                 info = paste("d =", d_val))
  }
})

test_that("sample_size_t_test finds correct n for 80% power", {
  result <- sample_size_t_test(d = 0.5, power = 0.80, type = "two.sample")
  expect_true(result$n >= 2)
  expect_true(result$achieved_power >= 0.79)
  # Verify self-consistency: power at n-1 should be < 0.80
  if (result$n > 2) {
    pw_below <- power_t_test(result$n - 1, d = 0.5, type = "two.sample")
    expect_true(pw_below < 0.80 || abs(pw_below - 0.80) < 0.01)
  }
})

test_that("power increases with sample size", {
  pw1 <- power_t_test(n = 10, d = 0.5, type = "two.sample")
  pw2 <- power_t_test(n = 50, d = 0.5, type = "two.sample")
  expect_true(pw2 > pw1)
})

test_that("power increases with effect size", {
  pw1 <- power_t_test(n = 30, d = 0.2, type = "two.sample")
  pw2 <- power_t_test(n = 30, d = 0.8, type = "two.sample")
  expect_true(pw2 > pw1)
})

test_that("sample_size_t_test round-trips with power_t_test", {
  result <- sample_size_t_test(d = 0.5, power = 0.85, type = "two.sample")
  pw_back <- power_t_test(result$n, d = 0.5, type = "two.sample")
  expect_equal(pw_back, result$achieved_power, tolerance = 1e-8)
})
