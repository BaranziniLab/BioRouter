# pca.R — PCA of samples

#' Compute PCA on a count matrix (samples as columns)
#'
#' Transposes the count matrix so PCA is computed on samples (observations)
#' rather than genes.
#'
#' @param counts Numeric matrix (genes x samples)
#' @param scale Whether to scale the data before PCA (default TRUE)
#' @param center Whether to center the data before PCA (default TRUE)
#' @param n_components Number of principal components to return
#' @return List with: coordinates (samples x PCs), var_explained, loadings
#' @export
compute_pca = function(counts, scale = TRUE, center = TRUE, n_components = NULL) {
  # Transpose: samples as rows, genes as columns
  t_counts = t(counts)

  # Replace any remaining NAs or Infs with 0
  t_counts[!is.finite(t_counts)] = 0

  # PCA
  pca_result = prcomp(t_counts, center = center, scale. = scale,
                      rank. = n_components)

  # Variance explained
  sdev = pca_result$sdev
  var_explained = sdev^2 / sum(sdev^2)

  # Coordinates
  coords = as.data.frame(pca_result$x)
  colnames(coords) = paste0("PC", seq_len(ncol(coords)))

  # Loadings
  loadings = as.data.frame(pca_result$rotation)
  colnames(loadings) = paste0("PC", seq_len(ncol(loadings)))

  list(
    coordinates = coords,
    var_explained = var_explained,
    loadings = loadings,
    sdev = sdev
  )
}

#' Summarize PCA results for reporting
#'
#' @param pca_result List from compute_pca
#' @param n_components Number of components to summarize
#' @return Data.frame with component, variance, cumulative_variance
#' @export
pca_summary = function(pca_result, n_components = NULL) {
  ve = pca_result$var_explained

  if (is.null(n_components)) {
    n_components = length(ve)
  }

  n_components = min(n_components, length(ve))

  data.frame(
    component = paste0("PC", seq_len(n_components)),
    variance = round(ve[seq_len(n_components)] * 100, 2),
    cumulative = round(cumsum(ve[seq_len(n_components)]) * 100, 2),
    stringsAsFactors = FALSE
  )
}
