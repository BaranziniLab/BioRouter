# Tests for power analysis

test_that("power_t_test is reasonable", {
  # With large effect and sample, power should be high
  result <- power_t_test(n = 50, d = 0.8, alpha = 0.05)
  expect_true(result$power > 0.9)

  # With small sample and effect, power should be low
  result2 <- power_t_test(n = 5, d = 0.2, alpha = 0.05)
  expect_true(result2$power < 0.5)
})

test_that("power increases with sample size", {
  p1 <- power_t_test(n = 10, d = 0.5)
  p2 <- power_t_test(n = 50, d = 0.5)
  p3 <- power_t_test(n = 100, d = 0.5)

  expect_true(p1$power < p2$power)
  expect_true(p2$power < p3$power)
})

test_that("power increases with effect size", {
  p1 <- power_t_test(n = 30, d = 0.2)
  p2 <- power_t_test(n = 30, d = 0.5)
  p3 <- power_t_test(n = 30, d = 0.8)

  expect_true(p1$power < p2$power)
  expect_true(p2$power < p3$power)
})

test_that("sample_size_t_test finds adequate n", {
  result <- sample_size_t_test(power = 0.80, d = 0.5, alpha = 0.05)

  expect_true(result$n >= 2)
  expect_true(result$power >= 0.80)

  # Check that n-1 would be insufficient
  result2 <- power_t_test(n = result$n - 1, d = 0.5)
  expect_true(result2$power < 0.80)
})

test_that("sample_size_t_test one-sided needs fewer subjects", {
  two_sided <- sample_size_t_test(power = 0.80, d = 0.5, alternative = "two.sided")
  one_sided <- sample_size_t_test(power = 0.80, d = 0.5, alternative = "one.sided")

  expect_true(one_sided$n <= two_sided$n)
})

test_that("power_anova is reasonable", {
  result <- power_anova(n = 20, k = 3, f = 0.3)
  expect_true(result$power > 0 && result$power <= 1)
})

test_that("sample_size_anova finds adequate n", {
  result <- sample_size_anova(power = 0.80, k = 3, f = 0.3)
  expect_true(result$n_per_group >= 2)
  expect_true(result$power >= 0.80)
})
