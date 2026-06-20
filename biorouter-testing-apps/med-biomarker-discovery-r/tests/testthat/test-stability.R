# ---- Tests for stability.R ----

cat("  stability.R tests\n")

test("select_features_stability basic", {
  set.seed(42)
  X <- matrix(rnorm(400), nrow = 40, ncol = 20)
  colnames(X) <- paste0("F", 1:20)
  y <- as.integer(X[, 1] + X[, 2] + rnorm(40, sd = 0.5) > 0)

  result <- select_features_stability(X, y,
                                       n_boot = 30,
                                       threshold = 0.5,
                                       lambda = 0.05, seed = 42)
  assert_true(is.list(result))
  assert_true("selected" %in% names(result))
  assert_true("frequency" %in% names(result))
  assert_equal(nrow(result$frequency), 20L)
  assert_true(all(result$frequency$frequency >= 0))
  assert_true(all(result$frequency$frequency <= 1))
})

test("stability frequency sums make sense", {
  set.seed(42)
  X <- matrix(rnorm(400), nrow = 40, ncol = 20)
  colnames(X) <- paste0("F", 1:20)
  y <- rep(0:1, each = 20)

  result <- select_features_stability(X, y, n_boot = 50,
                                       threshold = 0.5, seed = 42)
  # Frequencies should be reasonable
  assert_true(all(result$frequency$frequency >= 0))
  assert_true(all(result$frequency$frequency <= 1))
  # Features with high frequency should be selected
  high_freq <- result$frequency$feature[result$frequency$frequency >= 0.5]
  for (f in high_freq) {
    assert_true(f %in% result$selected)
  }
})

test("higher threshold selects fewer features", {
  set.seed(42)
  X <- matrix(rnorm(400), nrow = 40, ncol = 20)
  colnames(X) <- paste0("F", 1:20)
  y <- as.integer(X[, 1] + X[, 2] + rnorm(40, sd = 0.5) > 0)

  low <- select_features_stability(X, y, n_boot = 30,
                                    threshold = 0.3, seed = 42)
  high <- select_features_stability(X, y, n_boot = 30,
                                     threshold = 0.8, seed = 42)
  assert_gte(length(low$selected), length(high$selected))
})

cat("  stability.R tests complete.\n")
