# Tests for parametric tests - validated against base R

# ---- One-Sample t-test ----

test_that("hyp_one_sample_t matches t.test for known data", {
  set.seed(42)
  x <- c(5.2, 4.8, 5.5, 5.1, 4.9, 5.3, 5.0, 4.7, 5.4, 5.2)

  result <- hyp_one_sample_t(x, mu = 5.0)
  base <- t.test(x, mu = 5.0)

  expect_equal(unname(result$statistic), unname(base$statistic), tolerance = 1e-8)
  expect_equal(result$p_value, base$p.value, tolerance = 1e-8)
  expect_equal(unname(result$df), unname(base$parameter), tolerance = 1e-8)
})

test_that("hyp_one_sample_t matches t.test with alternative='less'", {
  set.seed(42)
  x <- rnorm(20, mean = 3, sd = 1)

  result <- hyp_one_sample_t(x, mu = 5.0, alternative = "less")
  base <- t.test(x, mu = 5.0, alternative = "less")

  expect_equal(result$p_value, base$p.value, tolerance = 1e-8)
})

test_that("hyp_one_sample_t matches t.test with alternative='greater'", {
  set.seed(42)
  x <- rnorm(20, mean = 7, sd = 1)

  result <- hyp_one_sample_t(x, mu = 5.0, alternative = "greater")
  base <- t.test(x, mu = 5.0, alternative = "greater")

  expect_equal(result$p_value, base$p.value, tolerance = 1e-8)
})

# ---- Two-Sample t-test ----

test_that("hyp_two_sample_t matches t.test for known data", {
  set.seed(42)
  x <- c(5.2, 4.8, 5.5, 5.1, 4.9, 5.3, 5.0, 4.7, 5.4, 5.2)
  y <- c(3.1, 3.5, 2.9, 3.3, 3.2, 3.4, 3.0, 3.6, 3.1, 3.3)

  result <- hyp_two_sample_t(x, y)
  base <- t.test(x, y, var.equal = TRUE)

  expect_equal(unname(result$statistic), unname(base$statistic), tolerance = 1e-8)
  expect_equal(result$p_value, base$p.value, tolerance = 1e-8)
  expect_equal(unname(result$df), unname(base$parameter), tolerance = 1e-8)
})

# ---- Paired t-test ----

test_that("hyp_paired_t matches t.test paired", {
  set.seed(42)
  x <- c(85, 90, 78, 92, 88, 76, 95, 89, 84, 91)
  y <- c(80, 85, 75, 88, 82, 72, 90, 85, 80, 87)

  result <- hyp_paired_t(x, y)
  base <- t.test(x, y, paired = TRUE)

  expect_equal(unname(result$statistic), unname(base$statistic), tolerance = 1e-8)
  expect_equal(result$p_value, base$p.value, tolerance = 1e-8)
})

# ---- Welch's t-test ----

test_that("hyp_welch_t matches t.test default (Welch)", {
  set.seed(42)
  x <- rnorm(15, mean = 0, sd = 1)
  y <- rnorm(20, mean = 0.5, sd = 2)

  result <- hyp_welch_t(x, y)
  base <- t.test(x, y)  # default is Welch

  expect_equal(result$p_value, base$p.value, tolerance = 1e-8)
})

# ---- One-Way ANOVA ----

test_that("hyp_one_way_anova matches aov for known data", {
  set.seed(42)
  df <- data.frame(
    value = c(rnorm(10, mean = 5), rnorm(10, mean = 6), rnorm(10, mean = 7)),
    group = factor(rep(c("A", "B", "C"), each = 10))
  )

  result <- hyp_one_way_anova(value ~ group, data = df)
  base <- aov(value ~ group, data = df)
  base_summary <- summary(base)

  # F-statistic
  expect_equal(result$statistic, unname(base_summary[[1]]$`F value`[1]), tolerance = 1e-8)
  # p-value
  expect_equal(result$p_value, unname(base_summary[[1]]$`Pr(>F)`[1]), tolerance = 1e-8)
  # df
  expect_equal(result$df[1], base_summary[[1]]$Df[1], tolerance = 1e-8)
  expect_equal(result$df[2], base_summary[[1]]$Df[2], tolerance = 1e-8)
})

# ---- F-test for Variances ----

test_that("hyp_f_test_variances matches var.test", {
  set.seed(42)
  x <- rnorm(30, sd = 1)
  y <- rnorm(30, sd = 1.5)

  result <- hyp_f_test_variances(x, y, alternative = "two.sided")
  base <- var.test(x, y)

  expect_equal(result$statistic, unname(base$statistic), tolerance = 1e-8)
  expect_equal(result$p_value, base$p.value, tolerance = 1e-8)
})

# ---- Pearson Correlation ----

test_that("hyp_pearson_r matches cor.test", {
  set.seed(42)
  x <- rnorm(30)
  y <- 2 * x + rnorm(30, sd = 0.5)

  result <- hyp_pearson_r(x, y)
  base <- cor.test(x, y)

  expect_equal(result$statistic, unname(base$estimate), tolerance = 1e-8)
  expect_equal(result$p_value, base$p.value, tolerance = 1e-8)
})

# ---- Simple Linear Regression ----

test_that("hyp_simple_regression matches lm", {
  set.seed(42)
  x <- 1:30
  y <- 2 + 0.5 * x + rnorm(30, sd = 2)

  result <- hyp_simple_regression(x, y)
  base <- lm(y ~ x)

  # R-squared
  expect_equal(result$extra$r_squared, summary(base)$r.squared, tolerance = 1e-8)
  # Coefficients
  expect_equal(result$extra$beta0, unname(coef(base)[1]), tolerance = 1e-8)
  expect_equal(result$extra$beta1, unname(coef(base)[2]), tolerance = 1e-8)
})

# ---- Multiple Linear Regression ----

test_that("hyp_multiple_regression matches lm", {
  set.seed(42)
  df <- data.frame(
    y = rnorm(50),
    x1 = rnorm(50),
    x2 = rnorm(50)
  )
  df$y <- 1 + 2 * df$x1 - 0.5 * df$x2 + rnorm(50, sd = 1)

  result <- hyp_multiple_regression(y ~ x1 + x2, data = df)
  base <- lm(y ~ x1 + x2, data = df)

  # R-squared
  expect_equal(result$extra$r_squared, summary(base)$r.squared, tolerance = 1e-8)
  # Overall F-test
  base_f <- summary(base)$fstatistic
  expect_equal(result$statistic, unname(base_f[1]), tolerance = 1e-8)
})
