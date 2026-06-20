library(testthat)
library(statSurvivalPower)

test_that("power_survival_logrank returns valid structure", {
  result <- power_survival_logrank(hr = 0.7, power = 0.80, alpha = 0.05)
  expect_true(is.list(result))
  expect_true("n_events_schoenfeld" %in% names(result))
  expect_true("n_events_freedman" %in% names(result))
  expect_true(result$n_events_schoenfeld > 0)
})

test_that("Schoenfeld formula matches reference values", {
  # Schoenfeld (1983): d = (z_a/2 + z_b)^2 * (1/p1 + 1/p2) / (log(HR))^2
  # For HR=0.7, alpha=0.05, power=0.80, equal allocation:
  result <- power_survival_logrank(hr = 0.7, power = 0.80, alpha = 0.05)
  z_a <- qnorm(0.975)
  z_b <- qnorm(0.80)
  expected_events <- (z_a + z_b)^2 / (log(0.7))^2 * 2
  expect_equal(result$n_events_schoenfeld, ceiling(expected_events), tolerance = 1)
})

test_that("Schoenfeld events increase with HR closer to 1", {
  r1 <- power_survival_logrank(hr = 0.5, power = 0.80)
  r2 <- power_survival_logrank(hr = 0.7, power = 0.80)
  r3 <- power_survival_logrank(hr = 0.9, power = 0.80)
  expect_true(r1$n_events_schoenfeld < r2$n_events_schoenfeld)
  expect_true(r2$n_events_schoenfeld < r3$n_events_schoenfeld)
})

test_that("Freedman events >= Schoenfeld events with accrual/followup", {
  result <- power_survival_logrank(
    hr = 0.7, power = 0.80, alpha = 0.05,
    t_accrual = 2, t_followup = 1
  )
  expect_true(result$n_events_freedman >= result$n_events_schoenfeld)
})

test_that("Dropout inflation increases events", {
  r0 <- power_survival_logrank(hr = 0.7, power = 0.80, dropout_rate = 0)
  r1 <- power_survival_logrank(hr = 0.7, power = 0.80, dropout_rate = 0.10)
  expect_true(r1$n_events_freedman >= r0$n_events_freedman)
})

test_that("sample_size_survival_logrank computes n_total", {
  result <- sample_size_survival_logrank(
    hr = 0.7, power = 0.80, t_accrual = 2, t_followup = 1
  )
  expect_true("n_total" %in% names(result))
  expect_true(result$n_total > 0)
  expect_true(result$n_total == 2 * result$n_per_arm)
})

test_that("Lower HR requires fewer events", {
  r1 <- power_survival_logrank(hr = 0.3, power = 0.80)
  r2 <- power_survival_logrank(hr = 0.7, power = 0.80)
  expect_true(r1$n_events_schoenfeld < r2$n_events_schoenfeld)
})
