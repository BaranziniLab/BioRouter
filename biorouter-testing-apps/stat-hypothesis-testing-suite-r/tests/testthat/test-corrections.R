# Tests for multiple comparison corrections

test_that("corr_bonferroni adjusts correctly", {
  p_vals <- c(0.01, 0.04, 0.03, 0.005, 0.10)
  result <- corr_bonferroni(p_vals)

  # Each p-value multiplied by number of tests
  expected <- p_vals * 5
  expected <- pmin(expected, 1)

  expect_equal(result$p_adjusted, expected, tolerance = 1e-10)
})

test_that("corr_bonferroni caps at 1", {
  p_vals <- c(0.5, 0.3, 0.4)
  result <- corr_bonferroni(p_vals)

  expect_true(all(result$p_adjusted <= 1))
})

test_that("corr_holm adjusts correctly", {
  p_vals <- c(0.01, 0.04, 0.03, 0.005, 0.10)
  result <- corr_holm(p_vals)

  # Holm should be less conservative than Bonferroni
  bonf <- corr_bonferroni(p_vals)

  # At least one adjusted p-value should be smaller than Bonferroni
  expect_true(any(result$p_adjusted <= bonf$p_adjusted + 1e-10))

  # All adjusted p-values should be in [0, 1]
  expect_true(all(result$p_adjusted >= 0 & result$p_adjusted <= 1))
})

test_that("corr_holm enforces monotonicity", {
  p_vals <- c(0.01, 0.04, 0.03, 0.005, 0.10)
  result <- corr_holm(p_vals)

  # Sort by raw p-value
  order_idx <- order(p_vals)
  sorted_adjusted <- result$p_adjusted[order_idx]

  # Should be non-decreasing
  for (i in 2:length(sorted_adjusted)) {
    expect_true(sorted_adjusted[i] >= sorted_adjusted[i-1] - 1e-10)
  }
})

test_that("corr_bh_fdr adjusts correctly", {
  p_vals <- c(0.01, 0.04, 0.03, 0.005, 0.10)
  result <- corr_bh_fdr(p_vals)

  # BH-FDR should be less conservative than Bonferroni
  bonf <- corr_bonferroni(p_vals)

  # At least one adjusted p-value should be smaller
  expect_true(any(result$p_adjusted <= bonf$p_adjusted + 1e-10))

  # All adjusted p-values should be in [0, 1]
  expect_true(all(result$p_adjusted >= 0 & result$p_adjusted <= 1))
})

test_that("corrections return correct significance decisions", {
  p_vals <- c(0.01, 0.04, 0.03, 0.005, 0.10)
  alpha <- 0.05

  bonf <- corr_bonferroni(p_vals, alpha)
  holm <- corr_holm(p_vals, alpha)
  bh <- corr_bh_fdr(p_vals, alpha)

  # Bonferroni should be most conservative
  n_bonf <- sum(bonf$significant)
  n_holm <- sum(holm$significant)
  n_bh <- sum(bh$significant)

  expect_true(n_bonf <= n_holm)
  expect_true(n_holm <= n_bh)
})
