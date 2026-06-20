#' Univariate screening for biomarker discovery
#'
#' Compute per-feature test statistics and p-values using t-tests, Wilcoxon
#' rank-sum tests, or correlation, with multiple-testing correction.

#' Screen features univariately
#'
#' @param X Numeric matrix (samples x features).
#' @param y Numeric outcome vector. If binary (2 levels), uses t-test / Wilcoxon;
#'   otherwise uses Pearson correlation.
#' @param method Character. One of "ttest", "wilcox", "correlation", "auto" (default "auto").
#'   "auto" picks ttest/wilcox for binary y, correlation for continuous.
#' @param correction Character vector of p-value adjustment methods.
#'   Default: c("bonferroni", "BH").
#' @param abs Logical. If TRUE (default for correlation), use absolute correlation.
#' @param sign Logical. If TRUE, return signed correlation (default FALSE).
#' @param min_abs_stat Numeric. Minimum absolute statistic to keep (default 0).
#' @return Data frame with columns:
#'   feature, statistic, pvalue, p_bonferroni, p_BH, direction (1/-1 for correlation).
#' @export
screen_univariate <- function(X, y,
                              method = c("auto", "ttest", "wilcox", "correlation"),
                              correction = c("bonferroni", "BH"),
                              abs = TRUE, sign = FALSE,
                              min_abs_stat = 0) {
  method <- match.arg(method)
  if (!is.matrix(X)) X <- as.matrix(X)
  n <- nrow(X)
  p <- ncol(X)
  feat_names <- colnames(X)
  if (is.null(feat_names)) feat_names <- paste0("feat_", seq_len(p))
  stopifnot(n == length(y))

  # Auto-select method
  if (method == "auto") {
    if (is_binary(y)) {
      method <- "wilcox"  # default to non-parametric for binary
    } else {
      method <- "correlation"
    }
  }

  stats <- numeric(p)
  pvals <- numeric(p)
  directions <- integer(p)

  if (method == "ttest" || method == "wilcox") {
    y_bin <- binarize(y)
    grp0 <- which(y_bin == 0)
    grp1 <- which(y_bin == 1)
    test_fn <- if (method == "ttest") t.test else wilcox.test
    for (j in seq_len(p)) {
      v <- X[, j]
      tt <- tryCatch(
        test_fn(v[grp1], v[grp0]),
        error = function(e) list(statistic = NA_real_, p.value = NA_real_)
      )
      stat <- tt$statistic
      if (is.list(stat)) stat <- stat[[1]]  # wilcox returns named list sometimes
      stats[j] <- as.numeric(stat)
      pvals[j] <- tt$p.value
      # Direction: positive stat means group 1 > group 0
      directions[j] <- if (!is.na(stats[j]) && stats[j] > 0) 1L else -1L
    }
  } else {
    # Correlation
    for (j in seq_len(p)) {
      cc <- tryCatch(
        cor(X[, j], y, use = "complete.obs"),
        error = function(e) NA_real_
      )
      stats[j] <- cc
      # Two-sided p-value from z-transform
      z <- cc * sqrt((n - 2) / (1 - cc^2))
      pvals[j] <- 2 * pnorm(-abs(z))
      directions[j] <- if (!is.na(cc) && cc > 0) 1L else -1L
    }
  }

  # Apply corrections
  result <- data.frame(
    feature   = feat_names,
    statistic = stats,
    pvalue    = pvals,
    direction = directions,
    stringsAsFactors = FALSE
  )

  for (m in correction) {
    adj <- p.adjust(pvals, method = m)
    result[[paste0("p_", m)]] <- adj
  }

  # Filter by min stat
  if (min_abs_stat > 0) {
    keep <- abs(result$statistic) >= min_abs_stat
    result <- result[keep, , drop = FALSE]
  }

  # Sort by p-value
  result <- result[order(result$pvalue), ]
  rownames(result) <- NULL
  result
}

#' Get significant features from univariate screen
#'
#' @param screen_df Data frame from screen_univariate.
#' @param alpha Significance level (default 0.05).
#' @param correction_method Which adjusted p-value column to use (default "p_BH").
#' @return Character vector of significant feature names.
#' @export
get_significant_features <- function(screen_df, alpha = 0.05,
                                     correction_method = "p_BH") {
  col_name <- correction_method
  if (!(col_name %in% names(screen_df))) {
    # fallback to pvalue
    col_name <- "pvalue"
  }
  sig <- screen_df[!is.na(screen_df[[col_name]]) & screen_df[[col_name]] <= alpha, ]
  sig$feature
}
