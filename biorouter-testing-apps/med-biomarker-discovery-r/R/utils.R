#' Utility functions for biomarkerDiscovR
#'
#' Internal helpers used across the package.

#' Safe matrix column-wise operation
#'
#' Applies a function to each column, returning NA for columns that error.
#' @param mat Numeric matrix.
#' @param fn Function to apply to each column vector.
#' @return Numeric vector of length ncol(mat).
#' @keywords internal
apply_cols <- function(mat, fn) {
  vapply(seq_len(ncol(mat)), function(j) {
    tryCatch(fn(mat[, j]), error = function(e) NA_real_)
  }, numeric(1))
}

#' Check binary outcome
#'
#' @param y Numeric vector.
#' @return Logical: TRUE if y has exactly 2 unique non-NA values.
#' @keywords internal
is_binary <- function(y) {
  length(unique(y[!is.na(y)])) == 2
}

#' Map a binary factor to 0/1
#'
#' @param y Factor or character/numeric with 2 levels.
#' @return Integer vector of 0s and 1s.
#' @keywords internal
binarize <- function(y) {
  lvls <- sort(unique(y[!is.na(y)]))
  if (length(lvls) != 2) stop("Expected exactly 2 levels.")
  as.integer(y == lvls[2])
}

#' Row-wise variance of a matrix
#'
#' @param X Numeric matrix (features in rows).
#' @return Numeric vector of length nrow(X).
#' @keywords internal
row_vars <- function(X) {
  apply(X, 1, var, na.rm = TRUE)
}

#' Column-wise variance of a matrix
#'
#' @param X Numeric matrix (features in columns).
#' @return Numeric vector of length ncol(X).
#' @keywords internal
col_vars <- function(X) {
  apply(X, 2, var, na.rm = TRUE)
}

#' Column-wise mean of a matrix
#'
#' @param X Numeric matrix (features in columns).
#' @return Numeric vector of length ncol(X).
#' @keywords internal
col_means <- function(X) {
  apply(X, 2, mean, na.rm = TRUE)
}

#' Robust z-score (median / MAD)
#'
#' @param x Numeric vector.
#' @return Numeric vector, same length.
#' @keywords internal
robust_z <- function(x) {
  m <- median(x, na.rm = TRUE)
  s <- mad(x, constant = 1.4826, na.rm = TRUE)
  if (s == 0) s <- 1
  (x - m) / s
}

#' Clip values to [lo, hi]
#'
#' @param x Numeric vector.
#' @param lo Lower bound.
#' @param hi Upper bound.
#' @return Numeric vector.
#' @keywords internal
clip <- function(x, lo = -Inf, hi = Inf) {
  pmax(lo, pmin(hi, x))
}

#' Check whether two integer / character vectors overlap meaningfully
#'
#' @param predicted Character vector of selected features.
#' @param truth Character vector of true features.
#' @return List with: overlap, precision, recall, f1.
#' @keywords internal
assess_selection <- function(predicted, truth) {
  tp <- length(intersect(predicted, truth))
  fp <- length(setdiff(predicted, truth))
  fn <- length(setdiff(truth, predicted))
  precision <- if (tp + fp > 0) tp / (tp + fp) else 0
  recall    <- if (tp + fn > 0) tp / (tp + fn) else 0
  f1 <- if (precision + recall > 0) 2 * precision * recall / (precision + recall) else 0
  list(overlap = tp, precision = precision, recall = recall, f1 = f1)
}

#' Compute AUC from labels and scores
#'
#' Simple trapezoidal AUC without any external dependency.
#' @param y_true Integer vector of 0/1 true labels.
#' @param scores Numeric vector of prediction scores (higher = more likely positive).
#' @return Scalar AUC in [0,1].
#' @keywords internal
compute_auc <- function(y_true, scores) {
  stopifnot(length(y_true) == length(scores))
  # remove NAs
  keep <- !is.na(y_true) & !is.na(scores)
  y_true <- y_true[keep]
  scores <- scores[keep]
  n_pos <- sum(y_true == 1)
  n_neg <- sum(y_true == 0)
  if (n_pos == 0 || n_neg == 0) return(NA_real_)
  # rank-based: proportion of pos-neg pairs where score_pos > score_neg
  pos_scores <- scores[y_true == 1]
  neg_scores <- scores[y_true == 0]
  # Handle ties via mid-rank
  tied <- 0
  higher <- 0
  for (ps in pos_scores) {
    higher <- higher + sum(ps > neg_scores)
    tied   <- tied   + sum(ps == neg_scores)
  }
  auc <- (higher + 0.5 * tied) / (n_pos * n_neg)
  auc
}

#' Compute accuracy from true labels and predicted class (majority vote of scores)
#'
#' @param y_true Integer 0/1.
#' @param scores Numeric scores.
#' @param threshold Numeric threshold (default 0.5).
#' @return Scalar accuracy in [0,1].
#' @keywords internal
compute_accuracy <- function(y_true, scores, threshold = 0.5) {
  keep <- !is.na(y_true) & !is.na(scores)
  y_true <- y_true[keep]
  scores <- scores[keep]
  pred <- as.integer(scores >= threshold)
  mean(pred == y_true)
}

#' K-fold indices
#'
#' @param n Number of samples.
#' @param k Number of folds.
#' @return List of k integer vectors (row indices).
#' @keywords internal
kfold_indices <- function(n, k = 5) {
  folds <- sample(rep(seq_len(k), length.out = n))
  lapply(seq_len(k), function(f) which(folds == f))
}

#' Shuffle matrix rows
#'
#' @param X Matrix or data.frame.
#' @return Shuffled version.
#' @keywords internal
shuffle_rows <- function(X) {
  X[sample(nrow(X)), , drop = FALSE]
}

#' Make feature name string
#'
#' @param prefix Character prefix.
#' @param i Integer index.
#' @return "prefix_001" style string.
#' @keywords internal
feature_name <- function(prefix, i) {
  sprintf("%s_%03d", prefix, i)
}
