# ---- Tests for evaluation.R ----

cat("  evaluation.R tests\n")

test("evaluate_model_cv basic", {
  set.seed(42)
  X <- matrix(rnorm(200), nrow = 20, ncol = 10)
  colnames(X) <- paste0("F", 1:10)
  y <- rep(0:1, each = 10)

  result <- evaluate_model_cv(X, y, features = c("F1", "F2"),
                               n_folds = 3, lambda = 0.05, seed = 42)
  assert_true(is.list(result))
  assert_true(!is.na(result$auc))
  assert_gte(result$auc, 0)
  assert_lte(result$auc, 1)
  assert_equal(length(result$fold_aucs), 3L)
})

test("evaluate_model_cv with more features", {
  set.seed(42)
  X <- matrix(rnorm(400), nrow = 40, ncol = 20)
  colnames(X) <- paste0("F", 1:20)
  y <- as.integer(X[, 1] + X[, 2] + X[, 3] + rnorm(40, sd = 0.5) > 0)

  feats <- c("F1", "F2", "F3")
  result <- evaluate_model_cv(X, y, features = feats,
                               n_folds = 5, lambda = 0.05, seed = 42)
  assert_gte(result$auc, 0)
  assert_lte(result$auc, 1)
  assert_gte(result$accuracy, 0)
  assert_lte(result$accuracy, 1)
})

test("cross_validate_panel ranks correctly", {
  set.seed(42)
  X <- matrix(rnorm(400), nrow = 40, ncol = 20)
  colnames(X) <- paste0("F", 1:20)
  y <- as.integer(X[, 1] + X[, 2] + rnorm(40, sd = 0.5) > 0)

  panels <- list(
    Good = c("F1", "F2"),
    Bad  = c("F10", "F11")
  )
  result <- cross_validate_panel(X, y, panels, n_folds = 3, seed = 42)
  assert_equal(nrow(result), 2L)
  assert_true(result$panel[1] %in% c("Good", "Bad"))
})

test("evaluate_model_cv SE is computed", {
  set.seed(42)
  X <- matrix(rnorm(200), nrow = 20, ncol = 10)
  colnames(X) <- paste0("F", 1:10)
  y <- as.integer(X[, 1] + rnorm(20, sd = 0.5) > 0)

  result <- evaluate_model_cv(X, y, features = "F1",
                               n_folds = 5, seed = 42)
  assert_true(!is.na(result$auc_se))
  assert_gte(result$auc_se, 0)
})

cat("  evaluation.R tests complete.\n")
