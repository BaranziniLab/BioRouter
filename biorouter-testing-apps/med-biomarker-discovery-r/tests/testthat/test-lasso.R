# ---- Tests for lasso.R ----

cat("  lasso.R tests\n")

test("fit_lasso basic", {
  set.seed(42)
  X <- matrix(rnorm(200), nrow = 20, ncol = 10)
  colnames(X) <- paste0("feat_", 1:10)
  y <- rep(0:1, each = 10)

  model <- fit_lasso(X, y, lambda = 0.1)
  assert_equal(length(model$beta), 10L)
  assert_true(is.numeric(model$intercept))
  assert_equal(model$lambda, 0.1)
  assert_equal(model$alpha, 1)
})

test("fit_lasso selects informative features", {
  set.seed(42)
  n <- 60
  X <- matrix(rnorm(n * 20), nrow = n, ncol = 20)
  colnames(X) <- paste0("F", 1:20)
  # Strong signal from F1, F2
  y <- as.integer(X[, 1] * 2 + X[, 2] * 2 + rnorm(n, sd = 0.3) > 0)

  # Scale features for LASSO
  X_scaled <- scale(X)
  model <- fit_lasso(X_scaled, y, lambda = 0.02)
  selected <- lasso_selected(model)
  # At least some true features should be selected
  overlap <- length(intersect(selected, c("F1", "F2")))
  assert_gte(overlap, 1L)
})

test("fit_lasso with high lambda selects fewer features", {
  set.seed(42)
  X <- matrix(rnorm(400), nrow = 40, ncol = 20)
  colnames(X) <- paste0("feat_", 1:20)
  y <- rep(0:1, each = 20)

  model_low <- fit_lasso(X, y, lambda = 0.01)
  model_high <- fit_lasso(X, y, lambda = 1.0)
  assert_gte(length(lasso_selected(model_low)),
             length(lasso_selected(model_high)))
})

test("predict_lasso returns probabilities", {
  set.seed(42)
  X <- matrix(rnorm(200), nrow = 20, ncol = 10)
  colnames(X) <- paste0("feat_", 1:10)
  y <- rep(0:1, each = 10)

  model <- fit_lasso(X, y, lambda = 0.1)
  preds <- predict_lasso(model, X)
  assert_equal(length(preds), 20L)
  # Probabilities should be in [0, 1]
  assert_gte(min(preds), -0.01)
  assert_lte(max(preds), 1.01)
})

test("lasso_selected returns correct feature names", {
  set.seed(42)
  X <- matrix(rnorm(200), nrow = 20, ncol = 10)
  colnames(X) <- paste0("feat_", 1:10)
  y <- rep(0:1, each = 10)

  model <- fit_lasso(X, y, lambda = 0.01)
  selected <- lasso_selected(model)
  # All selected features should be valid column names
  for (f in selected) {
    assert_in(f, colnames(X))
  }
})

test("elastic-net with alpha < 1 works", {
  set.seed(42)
  X <- matrix(rnorm(200), nrow = 20, ncol = 10)
  colnames(X) <- paste0("feat_", 1:10)
  y <- rep(0:1, each = 10)

  model <- fit_lasso(X, y, lambda = 0.1, alpha = 0.5)
  assert_equal(model$alpha, 0.5)
  assert_equal(length(model$beta), 10L)
})

test("fit_ridge works", {
  set.seed(42)
  X <- matrix(rnorm(200), nrow = 20, ncol = 10)
  colnames(X) <- paste0("feat_", 1:10)
  y <- rep(0:1, each = 10)

  model <- fit_ridge(X, y, lambda = 0.1)
  assert_equal(model$alpha, 0)
  assert_equal(length(model$beta), 10L)
  # Ridge should not zero out any coefficients
  assert_true(all(model$beta != 0))
})

cat("  lasso.R tests complete.\n")
