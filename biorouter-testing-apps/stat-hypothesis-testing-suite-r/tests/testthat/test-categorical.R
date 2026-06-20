# Tests for categorical tests

test_that("hyp_chi_square_gof matches chisq.test", {
  observed <- c(20, 30, 25, 25)
  expected <- c(25, 25, 25, 25)

  result <- hyp_chi_square_gof(observed, expected)
  base <- chisq.test(observed, p = rep(0.25, 4), correct = FALSE)

  expect_equal(result$statistic, unname(base$statistic), tolerance = 1e-8)
  expect_equal(result$p_value, base$p.value, tolerance = 1e-8)
})

test_that("hyp_chi_square_gof handles default expected", {
  observed <- c(10, 20, 30, 40)

  result <- hyp_chi_square_gof(observed)
  expect_equal(result$extra$expected, rep(25, 4))
})

test_that("hyp_chi_square_independence matches chisq.test", {
  # Larger contingency table (not 2x2, so no Yates correction)
  tbl <- matrix(c(10, 20, 30, 40, 15, 25, 35, 45), nrow = 2, ncol = 4,
                dimnames = list(c("A", "B"), c("X", "Y", "Z", "W")))

  result <- hyp_chi_square_independence(tbl)
  base <- chisq.test(tbl, correct = FALSE)

  expect_equal(result$statistic, unname(base$statistic), tolerance = 1e-8)
  expect_equal(result$p_value, base$p.value, tolerance = 1e-8)
})

test_that("hyp_chi_square_gof p-value is correct for known distribution", {
  # Expected uniform, observed matches expected
  observed <- c(25, 25, 25, 25)
  expected <- c(25, 25, 25, 25)

  result <- hyp_chi_square_gof(observed, expected)
  # Chi-square = 0, p should be 1
  expect_equal(result$statistic, 0)
  expect_equal(result$p_value, 1)
})

test_that("hyp_fisher_exact p-value matches fisher.test for 2x2 table", {
  tbl <- matrix(c(1, 5, 3, 8), nrow = 2)

  result <- hyp_fisher_exact(tbl, alternative = "two.sided")
  base <- fisher.test(tbl, alternative = "two.sided")

  expect_equal(result$p_value, base$p.value, tolerance = 1e-8)
})

test_that("hyp_fisher_exact one-sided works", {
  tbl <- matrix(c(1, 5, 3, 8), nrow = 2)

  result <- hyp_fisher_exact(tbl, alternative = "greater")
  base <- fisher.test(tbl, alternative = "greater")

  expect_equal(result$p_value, base$p.value, tolerance = 1e-8)
})

test_that("hyp_mcnemar basic test works", {
  # McNemar's test: discordant pairs
  # b=2, c=8 -> chi^2 = (2-8)^2/(2+8) = 36/10 = 3.6
  tbl <- matrix(c(10, 2, 8, 15), nrow = 2)

  result <- hyp_mcnemar(tbl)

  # Manual calculation: chi^2 = (b-c)^2/(b+c) = (2-8)^2/(2+8) = 3.6
  expect_equal(result$statistic, 3.6, tolerance = 1e-8)
  expect_true(result$p_value > 0 && result$p_value < 1)
})
