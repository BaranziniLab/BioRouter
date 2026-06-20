# ---- Tests for rfe.R ----

cat("  rfe.R tests\n")

test("recursive_feature_elimination basic", {
  set.seed(42)
  X <- matrix(rnorm(400), nrow = 40, ncol = 20)
  colnames(X) <- paste0("feat_", 1:20)
  y <- as.integer(X[, 1] + X[, 2] + rnorm(40, sd = 0.5) > 0)

  result <- recursive_feature_elimination(X, y,
                                           step_frac = 0.3,
                                           min_features = 5,
                                           lambda = 0.05, seed = 42)
  assert_true(is.list(result))
  assert_true("history" %in% names(result))
  assert_true("best_features" %in% names(result))
  assert_true("all_rankings" %in% names(result))
  assert_true(nrow(result$history) >= 1)
  assert_true(length(result$best_features) >= 5)
  assert_true(length(result$best_features) <= 20)
})

test("RFE produces multiple steps", {
  set.seed(42)
  X <- matrix(rnorm(400), nrow = 40, ncol = 20)
  colnames(X) <- paste0("feat_", 1:20)
  y <- rep(0:1, each = 20)

  result <- recursive_feature_elimination(X, y,
                                           step_frac = 0.25,
                                           min_features = 5,
                                           lambda = 0.05, seed = 42)
  assert_gte(nrow(result$history), 2L)
})

test("RFE history tracks AUC", {
  set.seed(42)
  X <- matrix(rnorm(400), nrow = 40, ncol = 20)
  colnames(X) <- paste0("feat_", 1:20)
  y <- as.integer(X[, 1] + rnorm(40, sd = 0.5) > 0)

  result <- recursive_feature_elimination(X, y,
                                           step_frac = 0.3,
                                           min_features = 8,
                                           lambda = 0.05, seed = 42)
  # All AUCs should be in valid range
  assert_true(all(result$history$auc >= 0 | is.na(result$history$auc)))
  assert_true(all(result$history$auc <= 1 | is.na(result$history$auc)))
})

test("RFE best_step is valid index", {
  set.seed(42)
  X <- matrix(rnorm(400), nrow = 40, ncol = 20)
  colnames(X) <- paste0("feat_", 1:20)
  y <- rep(0:1, each = 20)

  result <- recursive_feature_elimination(X, y,
                                           step_frac = 0.25,
                                           min_features = 5,
                                           lambda = 0.05, seed = 42)
  assert_gte(result$best_step, 1L)
  assert_lte(result$best_step, nrow(result$history))
})

cat("  rfe.R tests complete.\n")
