#' Stability Selection
#'
#' Repeatedly subsamples the data, fits LASSO, and selects features that
#' are consistently chosen across subsamples.

#' Stability Selection
#'
#' @param X Numeric matrix (samples x features).
#' @param y Integer vector 0/1.
#' @param n_boot Integer. Number of bootstrap / subsample iterations (default 100).
#' @param sample_frac Numeric in (0,1). Fraction of data per subsample (default 0.7).
#' @param lambda Numeric. LASSO penalty (default 0.05).
#' @param threshold Numeric in [0,1]. Selection frequency cutoff (default 0.7).
#' @param seed Integer. Random seed (default 42).
#' @return List with:
#'   \item{selected}{Character vector of features with frequency >= threshold.}
#'   \item{frequency}{Data frame: feature, frequency, selected (logical).}
#'   \item{threshold}{Used threshold.}
#' @export
select_features_stability <- function(X, y,
                                       n_boot = 100,
                                       sample_frac = 0.7,
                                       lambda = 0.05,
                                       threshold = 0.7,
                                       seed = 42) {
  if (!is.matrix(X)) X <- as.matrix(X)
  feat_names <- colnames(X)
  if (is.null(feat_names)) feat_names <- paste0("feat_", seq_len(ncol(X)))
  colnames(X) <- feat_names
  n <- nrow(X)
  p <- ncol(X)

  set.seed(seed)
  counts <- integer(p)
  names(counts) <- feat_names

  for (b in seq_len(n_boot)) {
    n_sub <- max(10, floor(n * sample_frac))
    idx <- sample(n, n_sub, replace = FALSE)
    X_sub <- X[idx, , drop = FALSE]
    y_sub <- y[idx]

    model <- tryCatch(
      fit_lasso(X_sub, y_sub, lambda = lambda),
      error = function(e) NULL
    )
    if (is.null(model)) next
    selected <- names(model$beta)[abs(model$beta) > 1e-10]
    counts[selected] <- counts[selected] + 1L
  }

  freq <- counts / n_boot
  freq_df <- data.frame(
    feature   = feat_names,
    frequency = as.numeric(freq[feat_names]),
    selected  = as.numeric(freq[feat_names]) >= threshold,
    stringsAsFactors = FALSE
  )
  freq_df <- freq_df[order(-freq_df$frequency), ]
  rownames(freq_df) <- NULL

  list(selected = feat_names[freq >= threshold],
       frequency = freq_df,
       threshold = threshold)
}
