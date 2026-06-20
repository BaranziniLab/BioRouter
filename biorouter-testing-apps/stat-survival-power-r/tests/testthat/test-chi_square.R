library(testthat)
library(statSurvivalPower)

test_that("power_chi_square returns valid power", {
  pw <- power_chi_square(n = 200, w = 0.3, df = 1)
  expect_true(pw > 0 && pw < 1)
  expect_length(pw, 1)
})

test_that("power_chi_square matches pwr::pwr.chisq.test", {
  skip_if_not_installed("pwr")
  for (w_val in c(0.1, 0.3, 0.5)) {
    for (n_val in c(50, 200, 500)) {
      ours <- power_chi_square(n = n_val, w = w_val, df = 1)
      theirs <- pwr::pwr.chisq.test(w = w_val, df = 1, N = n_val,
                                     sig.level = 0.05)$power
      expect_equal(ours, theirs, tolerance = 0.02,
                   info = paste("w =", w_val, "n =", n_val))
    }
  }
})

test_that("sample_size_chi_square finds n for 80% power", {
  result <- sample_size_chi_square(w = 0.3, df = 1, power = 0.80)
  expect_true(result$n >= 2)
  expect_true(result$achieved_power >= 0.79)
})

test_that("power increases with effect size w", {
  pw1 <- power_chi_square(n = 200, w = 0.1, df = 1)
  pw2 <- power_chi_square(n = 200, w = 0.5, df = 1)
  expect_true(pw2 > pw1)
})

test_that("sample_size round-trips", {
  result <- sample_size_chi_square(w = 0.4, df = 2, power = 0.90)
  pw_back <- power_chi_square(result$n, w = 0.4, df = 2)
  expect_equal(pw_back, result$achieved_power, tolerance = 1e-8)
})
