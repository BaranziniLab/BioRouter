# Tests for non-parametric tests

test_that("hyp_wilcoxon_rank_sum p-value matches wilcox.test", {
  set.seed(42)
  x <- c(5, 6, 7, 8, 9)
  y <- c(1, 2, 3, 4, 5)

  result <- hyp_wilcoxon_rank_sum(x, y)
  base <- wilcox.test(x, y, correct = FALSE)

  # p-values should be close (normal approximation vs exact)
  expect_equal(result$p_value, base$p.value, tolerance = 0.15)
})

test_that("hyp_wilcoxon_signed_rank p-value matches wilcox.test paired", {
  set.seed(42)
  x <- c(85, 90, 78, 92, 88, 76, 95, 89, 84, 91)
  y <- c(80, 85, 75, 88, 82, 72, 90, 85, 80, 87)

  result <- hyp_wilcoxon_signed_rank(x, y)
  base <- wilcox.test(x, y, paired = TRUE)

  expect_equal(result$p_value, base$p.value, tolerance = 0.15)
})

test_that("hyp_kruskal_wallis p-value matches kruskal.test", {
  set.seed(42)
  df <- data.frame(
    value = c(rnorm(10, mean = 5), rnorm(10, mean = 6), rnorm(10, mean = 7)),
    group = factor(rep(c("A", "B", "C"), each = 10))
  )

  result <- hyp_kruskal_wallis(value ~ group, data = df)
  base <- kruskal.test(value ~ group, data = df)

  # H statistic should match
  expect_equal(result$statistic, unname(base$statistic), tolerance = 0.01)
  expect_equal(result$p_value, base$p.value, tolerance = 0.1)
})

test_that("hyp_spearman_rho p-value matches cor.test method='spearman'", {
  set.seed(42)
  x <- 1:30
  y <- x + rnorm(30, sd = 3)

  result <- hyp_spearman_rho(x, y)
  base <- cor.test(x, y, method = "spearman", exact = FALSE)

  # rho should match
  expect_equal(result$statistic, unname(base$estimate), tolerance = 1e-8)
  expect_equal(result$p_value, base$p.value, tolerance = 0.15)
})

test_that("hyp_sign_test is correct", {
  x <- c(1, 2, 3, 4, 5)
  y <- c(0, 1, 2, 3, 4)
  # All differences are positive: x > y

  result <- hyp_sign_test(x, y)
  # 5 positive out of 5: exact binomial test
  base <- binom.test(5, 5, 0.5)
  expect_equal(result$p_value, base$p.value, tolerance = 0.01)
})

test_that("hyp_mann_whitney matches wilcox.test", {
  set.seed(42)
  x <- c(5, 6, 7, 8, 9, 10)
  y <- c(1, 2, 3, 4, 5, 6)

  result <- hyp_mann_whitney(x, y)
  base <- wilcox.test(x, y, correct = FALSE)

  expect_equal(result$p_value, base$p.value, tolerance = 0.15)
})
