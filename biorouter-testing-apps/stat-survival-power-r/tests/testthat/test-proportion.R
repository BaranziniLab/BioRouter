library(testthat)
library(statSurvivalPower)

test_that("power_two_proportion returns valid power", {
  pw <- power_two_proportion(n = 100, p1 = 0.30, p2 = 0.50)
  expect_true(pw > 0 && pw < 1)
  expect_length(pw, 1)
})

test_that("power_two_proportion matches pwr::pwr.2p.test", {
  skip_if_not_installed("pwr")
  # pwr uses h (arcsine effect size), so we compute h from proportions
  p1 <- 0.30; p2 <- 0.50
  h <- 2 * asin(sqrt(p1)) - 2 * asin(sqrt(p2))
  for (n_val in c(30, 80, 150)) {
    ours <- power_two_proportion(n = n_val, p1 = p1, p2 = p2)
    theirs <- pwr::pwr.2p.test(h = h, n = n_val, sig.level = 0.05)$power
    expect_equal(ours, theirs, tolerance = 0.02,
                 info = paste("n =", n_val))
  }
})

test_that("sample_size_two_proportion finds n for 80% power", {
  result <- sample_size_two_proportion(p1 = 0.30, p2 = 0.50, power = 0.80)
  expect_true(result$n >= 2)
  expect_true(result$achieved_power >= 0.79)
})

test_that("power increases with larger difference in proportions", {
  pw1 <- power_two_proportion(n = 50, p1 = 0.45, p2 = 0.50)
  pw2 <- power_two_proportion(n = 50, p1 = 0.20, p2 = 0.50)
  expect_true(pw2 > pw1)
})

test_that("sample_size round-trips", {
  result <- sample_size_two_proportion(p1 = 0.25, p2 = 0.45, power = 0.90)
  pw_back <- power_two_proportion(result$n, p1 = 0.25, p2 = 0.45)
  expect_equal(pw_back, result$achieved_power, tolerance = 1e-8)
})
