# ---- Tests for univariate.R ----

cat("  univariate.R tests\n")

test("screen_univariate with binary outcome (wilcox)", {
  set.seed(42)
  X <- matrix(rnorm(200), nrow = 20, ncol = 10)
  colnames(X) <- paste0("feat_", 1:10)
  # Make feat_1 truly different between groups
  X[1:10, 1] <- X[1:10, 1] + 5
  y <- rep(0:1, each = 10)

  result <- screen_univariate(X, y, method = "wilcox")
  assert_equal(nrow(result), 10L)
  # 6 columns: feature, statistic, pvalue, direction, p_bonferroni, p_BH
  assert_equal(ncol(result), 6L)
  # feat_1 should have smallest p-value
  assert_equal(result$feature[1], "feat_1")
  assert_true(result$pvalue[1] < 0.01)
})

test("screen_univariate with t-test", {
  set.seed(42)
  X <- matrix(rnorm(200), nrow = 20, ncol = 10)
  colnames(X) <- paste0("feat_", 1:10)
  X[1:10, 1] <- X[1:10, 1] + 5
  y <- rep(0:1, each = 10)

  result <- screen_univariate(X, y, method = "ttest")
  assert_equal(nrow(result), 10L)
  assert_equal(result$feature[1], "feat_1")
})

test("screen_univariate with continuous outcome (correlation)", {
  set.seed(42)
  X <- matrix(rnorm(200), nrow = 20, ncol = 10)
  colnames(X) <- paste0("feat_", 1:10)
  y <- X[, 1] * 2 + rnorm(20, sd = 0.1)

  result <- screen_univariate(X, y, method = "correlation")
  assert_equal(nrow(result), 10L)
  # feat_1 should have strongest correlation
  assert_equal(result$feature[1], "feat_1")
  assert_true(abs(result$statistic[1]) > 0.8)
})

test("screen_univariate auto method for binary", {
  set.seed(42)
  X <- matrix(rnorm(100), nrow = 20, ncol = 5)
  colnames(X) <- paste0("feat_", 1:5)
  y <- rep(0:1, each = 10)

  result <- screen_univariate(X, y, method = "auto")
  # Should use wilcox by default
  assert_true(nrow(result) == 5L)
})

test("screen_univariate auto method for continuous", {
  set.seed(42)
  X <- matrix(rnorm(100), nrow = 20, ncol = 5)
  colnames(X) <- paste0("feat_", 1:5)
  y <- rnorm(20)

  result <- screen_univariate(X, y, method = "auto")
  assert_true(nrow(result) == 5L)
})

test("multiple testing correction works", {
  set.seed(42)
  X <- matrix(rnorm(500), nrow = 50, ncol = 10)
  colnames(X) <- paste0("feat_", 1:10)
  y <- rep(0:1, each = 25)

  result <- screen_univariate(X, y, correction = c("bonferroni", "BH"))
  assert_true("p_bonferroni" %in% names(result))
  assert_true("p_BH" %in% names(result))
  # Bonferroni should always be >= raw p-value
  assert_true(all(result$p_bonferroni >= result$pvalue - 1e-15))
  # BH should also be >= raw p-value
  assert_true(all(result$p_BH >= result$pvalue - 1e-15))
})

test("get_significant_features works", {
  set.seed(42)
  X <- matrix(rnorm(500), nrow = 50, ncol = 10)
  colnames(X) <- paste0("feat_", 1:10)
  X[1:25, 1] <- X[1:25, 1] + 3
  y <- rep(0:1, each = 25)

  result <- screen_univariate(X, y)
  sig <- get_significant_features(result, alpha = 0.05)
  assert_true("feat_1" %in% sig)
})

test("screen_univariate direction is correct", {
  set.seed(42)
  X <- matrix(rnorm(100), nrow = 20, ncol = 5)
  colnames(X) <- paste0("feat_", 1:5)
  # feat_1: group 1 > group 0
  X[11:20, 1] <- X[11:20, 1] + 3
  y <- rep(0:1, each = 10)

  result <- screen_univariate(X, y, method = "wilcox")
  feat1_dir <- result$direction[result$feature == "feat_1"]
  assert_equal(feat1_dir, 1L)
})

test("screen_univariate min_abs_stat filter", {
  set.seed(42)
  X <- matrix(rnorm(1000), nrow = 50, ncol = 20)
  colnames(X) <- paste0("feat_", 1:20)
  y <- rep(0:1, each = 25)

  result_all <- screen_univariate(X, y, min_abs_stat = 0)
  result_filt <- screen_univariate(X, y, min_abs_stat = 1.0)
  assert_true(nrow(result_filt) <= nrow(result_all))
})

cat("  univariate.R tests complete.\n")
