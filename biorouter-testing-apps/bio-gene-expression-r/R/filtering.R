# filtering.R — Low-count gene filtering

#' Filter low-count genes from a count matrix
#'
#' Removes genes that do not meet minimum expression thresholds.
#' Default: genes must have at least 10 counts per million in at
#' least a minimum fraction of samples.
#'
#' @param counts Numeric matrix (genes x samples)
#' @param cpm_threshold CPM threshold (default 1)
#' @param min_samples Minimum number of samples meeting the CPM threshold
#' @param min_fraction If TRUE, interpret min_samples as a fraction of samples
#' @return Filtered count matrix
#' @export
filter_low_counts = function(counts,
                             cpm_threshold = 1,
                             min_samples = NULL,
                             min_fraction = TRUE) {

  nsamples = ncol(counts)

  if (is.null(min_samples)) {
    min_samples = ceiling(nsamples / 2)
  } else if (min_fraction && min_samples <= 1) {
    min_samples = ceiling(nsamples * min_samples)
  }

  # Compute CPM
  cpm = calculate_cpm(counts, log = FALSE)

  # Count samples passing threshold per gene
  passing = rowSums(cpm >= cpm_threshold)

  keep = passing >= min_samples

  counts_filtered = counts[keep, , drop = FALSE]

  message(sprintf("Filtering: %d -> %d genes (kept %.1f%%)",
                  nrow(counts), nrow(counts_filtered),
                  100 * nrow(counts_filtered) / nrow(counts)))

  counts_filtered
}

#' Filter genes by minimum total count across all samples
#'
#' @param counts Numeric matrix (genes x samples)
#' @param min_total Minimum total count across all samples
#' @return Filtered count matrix
#' @export
filter_by_total_counts = function(counts, min_total = 10) {
  totals = rowSums(counts)
  keep = totals >= min_total
  counts[keep, , drop = FALSE]
}
