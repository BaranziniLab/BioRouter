#' Model Evaluation via Cross-Validation
#'
#' Evaluate a feature panel by fitting a LASSO model on training folds
#' and computing AUC/accuracy on held-out folds.

#' Evaluate a feature panel by CV
#'
#' @param X Numeric matrix (samples x features).
#' @param y Integer vector 0/1.
#' @param features Character vector. Subset of colnames(X) to use.
#' @param n_folds Integer. Number of CV folds (default 5).
#' @param lambda Numeric. LASSO penalty (default 0.05).
#' @param seed Integer. Random seed (default 42).
#' @return List with:
#'   \item{auc}{Mean cross-validated AUC.}
#'   \item{auc_se}{Standard error of AUC across folds.}
#'   \item{accuracy}{Mean cross-validated accuracy.}
#'   \item{accuracy_se}{SE of accuracy.}
#'   \item{fold_aucs}{Numeric vector of per-fold AUCs.}
#'   \item{fold_accs}{Numeric vector of per-fold accuracies.}
#'   \item{features}{Used features.}
#' @export
evaluate_model_cv <- function(X, y, features,
                               n_folds = 5,
                               lambda = 0.05,
                               seed = 42) {
  if (!is.matrix(X)) X <- as.matrix(X)
  X_sub <- X[, features, drop = FALSE]
  n <- nrow(X_sub)

  set.seed(seed)
  folds <- kfold_indices(n, n_folds)

  fold_aucs  <- numeric(n_folds)
  fold_accs  <- numeric(n_folds)

  for (f in seq_len(n_folds)) {
    test_idx  <- folds[[f]]
    train_idx <- setdiff(seq_len(n), test_idx)

    model <- tryCatch(
      fit_lasso(X_sub[train_idx, , drop = FALSE],
                y[train_idx], lambda = lambda),
      error = function(e) NULL
    )
    if (is.null(model)) {
      fold_aucs[f] <- NA_real_
      fold_accs[f] <- NA_real_
      next
    }
    preds <- predict_lasso(model, X_sub[test_idx, , drop = FALSE])
    fold_aucs[f] <- compute_auc(y[test_idx], preds)
    fold_accs[f] <- compute_accuracy(y[test_idx], preds, threshold = 0.5)
  }

  list(auc = mean(fold_aucs, na.rm = TRUE),
       auc_se = sd(fold_aucs, na.rm = TRUE) / sqrt(sum(!is.na(fold_aucs))),
       accuracy = mean(fold_accs, na.rm = TRUE),
       accuracy_se = sd(fold_accs, na.rm = TRUE) / sqrt(sum(!is.na(fold_accs))),
       fold_aucs = fold_aucs,
       fold_accs = fold_accs,
       features = features)
}

#' Cross-validate and rank multiple panels
#'
#' Given a list of feature panels, evaluate each and rank by AUC.
#'
#' @param X Numeric matrix.
#' @param y Integer 0/1 vector.
#' @param panels Named list of character vectors (feature names).
#' @param n_folds Integer (default 5).
#' @param lambda Numeric (default 0.05).
#' @param seed Integer (default 42).
#' @return Data frame with columns: panel, n_features, auc, auc_se, accuracy, accuracy_se.
#' @export
cross_validate_panel <- function(X, y, panels,
                                  n_folds = 5,
                                  lambda = 0.05,
                                  seed = 42) {
  results <- data.frame(
    panel        = character(),
    n_features   = integer(),
    auc          = numeric(),
    auc_se       = numeric(),
    accuracy     = numeric(),
    accuracy_se  = numeric(),
    stringsAsFactors = FALSE
  )
  for (pname in names(panels)) {
    feats <- panels[[pname]]
    if (length(feats) == 0) next
    ev <- evaluate_model_cv(X, y, feats, n_folds = n_folds,
                            lambda = lambda, seed = seed)
    results <- rbind(results, data.frame(
      panel = pname, n_features = length(feats),
      auc = ev$auc, auc_se = ev$auc_se,
      accuracy = ev$accuracy, accuracy_se = ev$accuracy_se,
      stringsAsFactors = FALSE
    ))
  }
  results <- results[order(-results$auc), ]
  rownames(results) <- NULL
  results
}
