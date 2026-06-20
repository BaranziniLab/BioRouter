# utils.R — Helper/utility functions

#' Safe log2 transformation
#'
#' @param x Numeric vector or matrix
#' @param offset Offset added before log (default 1)
#' @return log2(x + offset)
#' @export
safe_log2 = function(x, offset = 1) {
  log2(pmax(x, 0) + offset)
}

#' Cross-tabulation of group membership
#'
#' @param groups Character/factor vector
#' @return Named integer vector of group counts
#' @export
count_groups = function(groups) {
  groups = as.factor(groups)
  tab = table(groups)
  as.integer(tab)
}

#' Check if a matrix has valid counts (non-negative integers)
#'
#' @param counts Numeric matrix
#' @return TRUE if valid; stops with error otherwise
#' @export
validate_counts = function(counts) {
  if (!is.matrix(counts)) {
    stop("Input must be a matrix")
  }
  if (any(counts < 0)) {
    stop("Count matrix contains negative values")
  }
  if (!is.numeric(counts)) {
    stop("Count matrix must be numeric")
  }
  invisible(TRUE)
}

#' Compute correlation distance between samples
#'
#' @param counts Numeric matrix (genes x samples)
#' @return Distance matrix
#' @export
sample_correlation_distance = function(counts) {
  cor_mat = cor(counts, use = "pairwise.complete.obs")
  as.dist(1 - cor_mat)
}

#' Hierarchical clustering of samples
#'
#' @param counts Numeric matrix (genes x samples)
#' @param method Clustering method (default "complete")
#' @return hclust object
#' @export
cluster_samples = function(counts, method = "complete") {
  dist = sample_correlation_distance(counts)
  hclust(dist, method = method)
}
