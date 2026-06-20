# ---- Tests for ranker.R ----

cat("  ranker.R tests\n")

test("rank_biomarker_panels basic", {
  set.seed(42)
  X <- matrix(rnorm(400), nrow = 40, ncol = 20)
  colnames(X) <- paste0("F", 1:20)
  y <- as.integer(X[, 1] + X[, 2] + rnorm(40, sd = 0.5) > 0)

  # Create mock screen
  screen <- data.frame(
    feature = paste0("F", 1:20),
    statistic = rnorm(20),
    pvalue = runif(20, 0, 0.1),
    direction = sample(c(-1, 1), 20, replace = TRUE),
    p_BH = runif(20, 0, 0.1),
    stringsAsFactors = FALSE
  )
  screen <- screen[order(screen$pvalue), ]

  # Create lasso model (beta already gets column names from fit_lasso)
  lasso_mod <- fit_lasso(scale(X), y, lambda = 0.05)

  result <- rank_biomarker_panels(
    X, y, screen_df = screen, lasso_model = lasso_mod,
    top_univariate = 10, n_folds = 3, seed = 42
  )

  assert_true(is.list(result))
  assert_true("ranking" %in% names(result))
  assert_true("panels" %in% names(result))
  assert_true(nrow(result$ranking) >= 2)
  assert_true(all(c("panel", "auc", "accuracy") %in% names(result$ranking)))
})

test("ranker produces ranked output", {
  set.seed(42)
  X <- matrix(rnorm(400), nrow = 40, ncol = 20)
  colnames(X) <- paste0("F", 1:20)
  y <- as.integer(X[, 1] + rnorm(40, sd = 0.5) > 0)

  screen <- data.frame(
    feature = paste0("F", 1:20),
    statistic = rnorm(20),
    pvalue = runif(20, 0, 0.1),
    direction = sample(c(-1, 1), 20, replace = TRUE),
    p_BH = runif(20, 0, 0.1),
    stringsAsFactors = FALSE
  )
  screen <- screen[order(screen$pvalue), ]

  result <- rank_biomarker_panels(
    X, y, screen_df = screen,
    top_univariate = 5, n_folds = 3, seed = 42
  )

  # Should be sorted by AUC descending
  aucs <- result$ranking$auc
  for (i in seq_len(length(aucs) - 1)) {
    assert_true(aucs[i] >= aucs[i + 1] - 0.01)
  }
})

cat("  ranker.R tests complete.\n")
