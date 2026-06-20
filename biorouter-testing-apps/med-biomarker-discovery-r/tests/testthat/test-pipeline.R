# ---- Tests for pipeline.R (integration) ----

cat("  pipeline.R integration tests\n")

test("pipeline runs end-to-end on small synthetic data", {
  data <- create_synthetic_data(n_samples = 100, n_features = 50,
                                 n_informative = 5, effect_size = 2.0,
                                 seed = 42)

  result <- pipeline(data$X, data$y,
                     lasso_lambda = 0.05,
                     n_cv_folds = 3,
                     n_stability_boot = 20,
                     seed = 42,
                     verbose = FALSE)

  assert_true(is.list(result))
  assert_true("screen" %in% names(result))
  assert_true("ranking" %in% names(result))
  assert_true("lasso_model" %in% names(result))
  assert_true(nrow(result$ranking$ranking) >= 2)
})

test("pipeline recovers some true features", {
  data <- create_synthetic_data(n_samples = 100, n_features = 50,
                                 n_informative = 5, effect_size = 2.0,
                                 seed = 42)

  result <- pipeline(data$X, data$y,
                     lasso_lambda = 0.05,
                     n_cv_folds = 3,
                     n_stability_boot = 20,
                     seed = 42,
                     verbose = FALSE)

  # Get features from best panel
  best_feats <- result$ranking$ranking$features[[1]]
  # At least 1 true feature should be in the best panel
  overlap <- length(intersect(best_feats, data$true_features))
  assert_gte(overlap, 1L)
})

test("pipeline screen has reasonable BH p-values", {
  data <- create_synthetic_data(n_samples = 100, n_features = 50,
                                 n_informative = 5, effect_size = 2.0,
                                 seed = 42)

  result <- pipeline(data$X, data$y,
                     lasso_lambda = 0.05,
                     n_cv_folds = 3,
                     n_stability_boot = 20,
                     seed = 42,
                     verbose = FALSE)

  screen <- result$screen
  assert_true("p_BH" %in% names(screen))
  # True features should have small p-values
  true_pvals <- screen$p_BH[screen$feature %in% data$true_features]
  assert_true(any(true_pvals < 0.1))
})

test("pipeline ranking is sorted by AUC", {
  data <- create_synthetic_data(n_samples = 100, n_features = 50,
                                 n_informative = 5, effect_size = 2.0,
                                 seed = 42)

  result <- pipeline(data$X, data$y,
                     lasso_lambda = 0.05,
                     n_cv_folds = 3,
                     n_stability_boot = 20,
                     seed = 42,
                     verbose = FALSE)

  aucs <- result$ranking$ranking$auc
  # Should be non-increasing (sorted descending)
  for (i in seq_len(length(aucs) - 1)) {
    assert_true(aucs[i] >= aucs[i + 1] - 0.001)
  }
})

cat("  pipeline.R integration tests complete.\n")
