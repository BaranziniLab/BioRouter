#' Preprocessing for high-dimensional biomarker data
#'
#' Functions for filtering low-variance features, handling missing values,
#' and normalizing a features-by-samples matrix.

#' Preprocess a feature matrix
#'
#' @param X Numeric matrix (samples in rows, features in columns).
#' @param y Optional numeric outcome vector (length nrow(X)).
#' @param var_threshold Numeric. Features with variance below this are removed (default 0.01).
#' @param missing_threshold Numeric. Features with fraction missing above this are removed (default 0.3).
#' @param norm_method Character. One of "zscore", "robust_z", "minmax", or "none" (default "zscore").
#' @param impute Character. One of "median", "mean", "zero" (default "median").
#' @param center Logical. Center features? (default TRUE).
#' @param scale Logical. Scale features? (default TRUE).
#' @return List with components:
#'   \item{X}{Cleaned, normalized matrix.}
#'   \item{y}{Outcome vector (if provided).}
#'   \item{removed_var}{Names of features removed by variance filter.}
#'   \item{removed_miss}{Names of features removed by missing filter.}
#'   \item{impute_values}{Named list of imputation values.}
#'   \item{norm_params}{List with mean/sd or median/mad per retained feature.}
#'   \item{retained}{Character vector of retained feature names.}
#' @export
preprocess_data <- function(X, y = NULL,
                            var_threshold = 0.01,
                            missing_threshold = 0.3,
                            norm_method = c("zscore", "robust_z", "minmax", "none"),
                            impute = c("median", "mean", "zero"),
                            center = TRUE, scale = TRUE) {
  norm_method <- match.arg(norm_method)
  impute <- match.arg(impute)

  if (!is.matrix(X)) X <- as.matrix(X)
  feat_names <- colnames(X)
  if (is.null(feat_names)) feat_names <- paste0("V", seq_len(ncol(X)))
  colnames(X) <- feat_names
  sample_names <- rownames(X)
  if (is.null(sample_names)) sample_names <- paste0("S", seq_len(nrow(X)))
  rownames(X) <- sample_names

  # --- Missing-value filter ---
  miss_frac <- colMeans(is.na(X))
  removed_miss <- feat_names[miss_frac > missing_threshold]
  keep_miss <- miss_frac <= missing_threshold
  X <- X[, keep_miss, drop = FALSE]
  feat_names <- colnames(X)

  # --- Imputation ---
  impute_values <- list()
  for (j in seq_len(ncol(X))) {
    col <- X[, j]
    if (impute == "median") {
      val <- median(col, na.rm = TRUE)
    } else if (impute == "mean") {
      val <- mean(col, na.rm = TRUE)
    } else {
      val <- 0
    }
    if (is.na(val)) val <- 0
    impute_values[[feat_names[j]]] <- val
    X[is.na(X[, j]), j] <- val
  }

  # --- Variance filter ---
  v <- col_vars(X)
  removed_var <- feat_names[v < var_threshold]
  keep_var <- v >= var_threshold
  X <- X[, keep_var, drop = FALSE]
  feat_names <- colnames(X)

  # --- Normalization ---
  norm_params <- list(method = norm_method, center = center, scale = scale)
  if (norm_method == "zscore") {
    mu  <- col_means(X)
    sds <- apply(X, 2, sd)
    sds[sds == 0] <- 1
    norm_params$center_vals <- mu
    norm_params$scale_vals  <- sds
    if (center) X <- sweep(X, 2, mu)
    if (scale)  X <- sweep(X, 2, sds, "/")
  } else if (norm_method == "robust_z") {
    med <- apply(X, 2, median)
    mads <- apply(X, 2, mad, constant = 1.4826)
    mads[mads == 0] <- 1
    norm_params$center_vals <- med
    norm_params$scale_vals  <- mads
    if (center) X <- sweep(X, 2, med)
    if (scale)  X <- sweep(X, 2, mads, "/")
  } else if (norm_method == "minmax") {
    lo <- apply(X, 2, min)
    hi <- apply(X, 2, max)
    rng <- hi - lo
    rng[rng == 0] <- 1
    norm_params$center_vals <- lo
    norm_params$scale_vals  <- rng
    if (center) X <- sweep(X, 2, lo)
    if (scale)  X <- sweep(X, 2, rng, "/")
  }

  list(X = X, y = y,
       removed_var = removed_var, removed_miss = removed_miss,
       impute_values = impute_values, norm_params = norm_params,
       retained = colnames(X))
}
