#' Recursive Feature Elimination (RFE)
#'
#' Iteratively removes the least important features using model-based
#' importance (e.g., |coefficient| from LASSO).

#' Recursive Feature Elimination
#'
#' Fits a model, ranks features by importance, removes the bottom fraction,
#' and repeats until the desired number of features is reached.
#'
#' @param X Numeric matrix (samples x features).
#' @param y Integer vector 0/1 (binary outcome).
#' @param step_frac Numeric in (0,1). Fraction of features to remove each step (default 0.2).
#' @param min_features Integer. Stop when this many features remain (default 5).
#' @param n_folds Integer. Internal CV folds for importance estimation (default 5).
#' @param lambda Numeric. LASSO penalty (default 0.05).
#' @param seed Integer. Random seed (default 42).
#' @return List with:
#'   \item{history}{Data frame: step, n_features, auc.}
#'   \item{best_features}{Character vector of features at best step.}
#'   \item{best_step}{Integer step index.}
#'   \item{all_rankings}{Data frame: feature, rank, avg_coef.}
#' @export
recursive_feature_elimination <- function(X, y,
                                           step_frac = 0.2,
                                           min_features = 5,
                                           n_folds = 5,
                                           lambda = 0.05,
                                           seed = 42) {
  if (!is.matrix(X)) X <- as.matrix(X)
  feat_names <- colnames(X)
  if (is.null(feat_names)) feat_names <- paste0("feat_", seq_len(ncol(X)))
  colnames(X) <- feat_names
  n <- nrow(X)
  p <- ncol(X)
  step_frac <- max(0.05, min(step_frac, 0.5))

  # Accumulated ranking (lower = more important)
  rank_sum <- numeric(p)
  names(rank_sum) <- feat_names
  n_ranks <- integer(p)
  names(n_ranks) <- feat_names

  active <- feat_names
  history <- data.frame(step = integer(), n_features = integer(),
                        auc = numeric(), stringsAsFactors = FALSE)
  best_auc <- -Inf
  best_features <- active
  best_step <- 0L
  step <- 0L

  while (length(active) >= min_features) {
    step <- step + 1
    X_active <- X[, active, drop = FALSE]

    # Estimate feature importance via LASSO across CV folds
    set.seed(seed + step)
    folds <- kfold_indices(n, n_folds)
    coef_accum <- numeric(length(active))
    names(coef_accum) <- active
    auc_accum <- numeric(n_folds)

    for (f in seq_len(n_folds)) {
      test_idx  <- folds[[f]]
      train_idx <- setdiff(seq_len(n), test_idx)
      model <- tryCatch(
        fit_lasso(X_active[train_idx, , drop = FALSE],
                  y[train_idx], lambda = lambda),
        error = function(e) NULL
      )
      if (is.null(model)) next
      coef_accum[active] <- coef_accum[active] + abs(model$beta)
      # CV AUC for this fold
      preds <- predict_lasso(model, X_active[test_idx, , drop = FALSE])
      auc_accum[f] <- compute_auc(y[test_idx], preds)
    }

    avg_coef <- coef_accum / n_folds
    avg_auc  <- mean(auc_accum, na.rm = TRUE)

    # Record
    history <- rbind(history, data.frame(step = step, n_features = length(active),
                                         auc = avg_auc, stringsAsFactors = FALSE))
    if (avg_auc > best_auc) {
      best_auc <- avg_auc
      best_features <- active
      best_step <- step
    }

    # Update rankings
    ranks <- rank(-avg_coef, ties.method = "average")
    for (fname in active) {
      rank_sum[fname] <- rank_sum[fname] + ranks[fname]
      n_ranks[fname]  <- n_ranks[fname] + 1
    }

    # Determine how many to remove
    n_remove <- max(1, floor(length(active) * step_frac))
    # Remove least important
    to_remove <- names(sort(avg_coef))[seq_len(n_remove)]
    active <- setdiff(active, to_remove)
  }

  # Final rankings
  avg_rank <- ifelse(n_ranks > 0, rank_sum / n_ranks, Inf)
  all_rankings <- data.frame(
    feature  = feat_names,
    rank     = avg_rank,
    avg_coef = ifelse(n_ranks > 0, rank_sum / n_ranks, 0),
    stringsAsFactors = FALSE
  )
  all_rankings <- all_rankings[order(all_rankings$rank), ]

  list(history = history,
       best_features = best_features,
       best_step = best_step,
       all_rankings = all_rankings)
}
