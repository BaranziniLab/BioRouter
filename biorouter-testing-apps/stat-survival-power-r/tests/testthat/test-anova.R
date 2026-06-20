library(testthat)
library(statSurvivalPower)

test_that("power_anova returns valid power", {
  pw <- power_anova(n = 20, k = 3, f = 0.25)
  expect_true(pw > 0 && pw < 1)
  expect_length(pw, 1)
})

test_that("power_anova matches pwr::pwr.anova.test", {
  skip_if_not_installed("pwr")
  for (f_val in c(0.15, 0.25, 0.40)) {
    for (n_val in c(10, 25, 50)) {
      ours <- power_anova(n = n_val, k = 3, f = f_val)
      theirs <- pwr::pwr.anova.test(k = 3, n = n_val, f = f_val,
                                     sig.level = 0.05)$power
      expect_equal(ours, theirs, tolerance = 0.01,
                   info = paste("f =", f_val, "n =", n_val))
    }
  }
})

test_that("sample_size_anova finds n for 80% power", {
  result <- sample_size_anova(k = 3, f = 0.25, power = 0.80)
  expect_true(result$n >= 2)
  expect_true(result$achieved_power >= 0.79)
  # Self-consistency
  pw_below <- power_anova(result$n - 1, k = 3, f = 0.25)
  expect_true(pw_below < 0.80 || abs(pw_below - 0.80) < 0.01)
})

test_that("power_anova increases with f", {
  pw1 <- power_anova(n = 20, k = 3, f = 0.1)
  pw2 <- power_anova(n = 20, k = 3, f = 0.5)
  expect_true(pw2 > pw1)
})

test_that("sample_size_anova round-trips", {
  result <- sample_size_anova(k = 4, f = 0.3, power = 0.90)
  pw_back <- power_anova(result$n, k = 4, f = 0.3)
  expect_equal(pw_back, result$achieved_power, tolerance = 1e-8)
})
