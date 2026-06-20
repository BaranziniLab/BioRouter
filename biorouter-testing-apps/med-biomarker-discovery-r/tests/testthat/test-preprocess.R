# ---- Tests for preprocess.R ----

cat("  preprocess.R tests\n")

test("preprocess_data basic functionality", {
  set.seed(42)
  X <- matrix(rnorm(200), nrow = 20, ncol = 10)
  colnames(X) <- paste0("feat_", 1:10)
  y <- rep(0:1, each = 10)

  result <- preprocess_data(X, y)
  assert_true(is.matrix(result$X))
  assert_equal(ncol(result$X), 10L)
  assert_equal(nrow(result$X), 20L)
  assert_equal(length(result$y), 20L)
  assert_true(length(result$retained) <= 10L)
})

test("preprocess_data filters low-variance features", {
  set.seed(42)
  X <- matrix(rnorm(200), nrow = 20, ncol = 10)
  colnames(X) <- paste0("feat_", 1:10)
  # Make feat_1 constant (zero variance)
  X[, 1] <- 5.0
  y <- rep(0:1, each = 10)

  result <- preprocess_data(X, y, var_threshold = 0.01)
  assert_true("feat_1" %in% result$removed_var)
  assert_false("feat_1" %in% result$retained)
  assert_equal(ncol(result$X), 9L)
})

test("preprocess_data handles missing values", {
  set.seed(42)
  X <- matrix(rnorm(200), nrow = 20, ncol = 10)
  colnames(X) <- paste0("feat_", 1:10)
  y <- rep(0:1, each = 10)

  # Set 50% of feat_1 to NA
  X[1:10, 1] <- NA
  result <- preprocess_data(X, y, missing_threshold = 0.3)
  assert_true("feat_1" %in% result$removed_miss)
})

test("preprocess_data imputation works", {
  set.seed(42)
  X <- matrix(rnorm(200), nrow = 20, ncol = 10)
  colnames(X) <- paste0("feat_", 1:10)
  X[1, 3] <- NA
  X[5, 3] <- NA
  y <- rep(0:1, each = 10)

  result <- preprocess_data(X, y, missing_threshold = 1.0)
  # No NAs in output
  assert_true(all(!is.na(result$X)))
})

test("preprocess_data zscore normalization", {
  set.seed(42)
  X <- matrix(rnorm(100) * 10 + 5, nrow = 20, ncol = 5)
  colnames(X) <- paste0("feat_", 1:5)
  y <- rep(0:1, each = 10)

  result <- preprocess_data(X, y, norm_method = "zscore")
  # After zscore, means should be approximately 0
  means <- col_means(result$X)
  assert_true(all(abs(means) < 0.2))
})

test("preprocess_data robust_z normalization", {
  set.seed(42)
  X <- matrix(rnorm(100) * 10 + 5, nrow = 20, ncol = 5)
  colnames(X) <- paste0("feat_", 1:5)
  y <- rep(0:1, each = 10)

  result <- preprocess_data(X, y, norm_method = "robust_z")
  assert_true(is.matrix(result$X))
})

test("preprocess_data minmax normalization", {
  set.seed(42)
  X <- matrix(rnorm(100) * 10 + 5, nrow = 20, ncol = 5)
  colnames(X) <- paste0("feat_", 1:5)
  y <- rep(0:1, each = 10)

  result <- preprocess_data(X, y, norm_method = "minmax")
  # After minmax, values should be in [0, 1]
  assert_gte(min(result$X), -0.01)
  assert_lte(max(result$X), 1.01)
})

test("preprocess_data mean imputation", {
  X <- matrix(1:20, nrow = 5, ncol = 4)
  X[1, 1] <- NA
  colnames(X) <- paste0("feat_", 1:4)
  y <- c(0, 0, 1, 1, 1)

  # Use norm_method="none" to avoid normalization changing values
  result <- preprocess_data(X, y, impute = "mean", missing_threshold = 1.0,
                            norm_method = "none")
  # matrix(1:20,5,4) fills column-major: col1 = 1,2,3,4,5 so mean of rows 2-5 = 3.5
  expected_mean <- mean(c(2, 3, 4, 5))
  assert_equal(result$X[1, 1], expected_mean)
})

test("preprocess_data with no removal", {
  set.seed(42)
  X <- matrix(rnorm(100), nrow = 20, ncol = 5)
  colnames(X) <- paste0("feat_", 1:5)
  y <- rep(0:1, each = 10)

  result <- preprocess_data(X, y, var_threshold = 0, missing_threshold = 1.0,
                            norm_method = "none")
  assert_equal(ncol(result$X), 5L)
  assert_equal(length(result$removed_var), 0L)
  assert_equal(length(result$removed_miss), 0L)
})

cat("  preprocess.R tests complete.\n")
