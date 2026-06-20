library(testthat)
library(statSurvivalPower)

test_that("solve_power finds correct n for target power", {
  result <- solve_power(power_t_test, target = "n", target_value = 0.80,
                        d = 0.5, type = "two.sample")
  expect_true(result$found_value >= 2)
  expect_equal(result$achieved_value, 0.80, tolerance = 0.01)
})

test_that("solve_power finds correct effect size for target power", {
  result <- solve_power(power_t_test, target = "d", target_value = 0.80,
                        n = 30, type = "two.sample", hi = 5.0)
  expect_true(result$found_value > 0)
  expect_equal(result$achieved_value, 0.80, tolerance = 0.01)
  # Cross-check
  pw <- power_t_test(n = 30, d = result$found_value, type = "two.sample")
  expect_equal(pw, 0.80, tolerance = 0.01)
})

test_that("solve_power is self-consistent across functions", {
  # ANOVA: solve for n
  result <- solve_power(power_anova, target = "n", target_value = 0.80,
                        k = 3, f = 0.25)
  pw <- power_anova(result$found_value, k = 3, f = 0.25)
  expect_equal(pw, 0.80, tolerance = 0.01)
})

test_that("solve_power for alpha works", {
  result <- solve_power(power_t_test, target = "alpha", target_value = 0.80,
                        n = 30, d = 0.5, type = "two.sample", lo = 0.001, hi = 0.20)
  # At the solved alpha, power should be ~0.80
  pw <- power_t_test(n = 30, d = 0.5, alpha = result$found_value, type = "two.sample")
  expect_equal(pw, 0.80, tolerance = 0.02)
})

test_that("sample_size_t_test agrees with solve_power for n", {
  r1 <- sample_size_t_test(d = 0.5, power = 0.80, type = "two.sample")
  r2 <- solve_power(power_t_test, target = "n", target_value = 0.80,
                    d = 0.5, type = "two.sample")
  # They should be very close (within a few units due to discrete n)
  expect_true(abs(r1$n - r2$found_value) <= 2)
})
