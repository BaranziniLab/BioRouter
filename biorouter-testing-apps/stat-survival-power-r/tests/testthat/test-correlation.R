library(testthat)
library(statSurvivalPower)

test_that("power_correlation returns valid power", {
  pw <- power_correlation(n = 50, r = 0.3)
  expect_true(pw > 0 && pw < 1)
  expect_length(pw, 1)
})

test_that("power_correlation matches pwr::pwr.r.test", {
  skip_if_not_installed("pwr")
  for (r_val in c(0.2, 0.4, 0.6)) {
    for (n_val in c(20, 50, 100)) {
      ours <- power_correlation(n = n_val, r = r_val)
      theirs <- pwr::pwr.r.test(n = n_val, r = r_val, sig.level = 0.05)$power
      expect_equal(ours, theirs, tolerance = 0.02,
                   info = paste("r =", r_val, "n =", n_val))
    }
  }
})

test_that("sample_size_correlation finds n for 80% power", {
  result <- sample_size_correlation(r = 0.3, power = 0.80)
  expect_true(result$n >= 5)
  expect_true(result$achieved_power >= 0.79)
})

test_that("power increases with correlation magnitude", {
  pw1 <- power_correlation(n = 50, r = 0.1)
  pw2 <- power_correlation(n = 50, r = 0.5)
  expect_true(pw2 > pw1)
})

test_that("sample_size round-trips", {
  result <- sample_size_correlation(r = 0.4, power = 0.85)
  pw_back <- power_correlation(result$n, r = 0.4)
  expect_equal(pw_back, result$achieved_power, tolerance = 1e-8)
})
