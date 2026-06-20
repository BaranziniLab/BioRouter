# normalization.R — Library-size normalization methods

#' Calculate Counts Per Million (CPM)
#'
#' @param counts Numeric matrix (genes x samples)
#' @param log If TRUE, returns log2(CPM + 1)
#' @return Matrix of same dimensions with CPM values
#' @export
calculate_cpm = function(counts, log = FALSE) {
  lib_sizes = colSums(counts)
  # Avoid division by zero
  lib_sizes[lib_sizes == 0] = 1
  cpm = sweep(counts, 2, lib_sizes / 1e6, "/")

  if (log) {
    cpm = log2(cpm + 1)
  }

  cpm
}

#' Calculate TMM-like scaling factors (simplified Robinson & Oshlack)
#'
#' Computes a trimmed mean of M-values (TMM) between each sample and
#' a reference sample (the one whose upper quartile is closest to the
#' mean upper quartile).
#'
#' @param counts Numeric matrix (genes x samples)
#' @param ref_column Optional index of the reference column
#' @param trim Fraction to trim from each tail of the M-value distribution
#' @return Named numeric vector of scaling factors (one per sample)
#' @export
calculate_tmm_factors = function(counts, ref_column = NULL, trim = 0.3) {
  nsamples = ncol(counts)

  if (nsamples == 1) {
    return(setNames(1.0, colnames(counts)[1]))
  }

  # Find reference: sample whose upper-quartile log-ratio is closest to median
  if (is.null(ref_column)) {
    lib_sizes = colSums(counts)
    lib_sizes[lib_sizes == 0] = 1
    log_lib = log(lib_sizes)
    ref_column = which.min(abs(log_lib - median(log_lib)))
  }

  factors = numeric(nsamples)
  ref = counts[, ref_column]
  ref_lib = sum(ref)
  ref_freq = ref / ref_lib
  ref_freq[ref_freq == 0] = .Machine$double.xmin

  for (j in seq_len(nsamples)) {
    if (j == ref_column) {
      factors[j] = 1.0
      next
    }

    sample = counts[, j]
    sample_lib = sum(sample)
    sample_freq = sample / sample_lib
    sample_freq[sample_freq == 0] = .Machine$double.xmin

    # M-values: log2(frequency ratio)
    m_vals = log2(sample_freq / ref_freq)

    # A-values: average log2 frequency
    a_vals = (log2(sample_freq) + log2(ref_freq)) / 2

    # Filter out extreme values
    keep = is.finite(m_vals) & is.finite(a_vals)

    # Trim from both tails
    q_lo = quantile(a_vals[keep], probs = trim, na.rm = TRUE)
    q_hi = quantile(a_vals[keep], probs = 1 - trim, na.rm = TRUE)
    keep = keep & a_vals >= q_lo & a_vals <= q_hi

    # Trimmed mean of M-values
    tmm = mean(m_vals[keep], na.rm = TRUE)

    # Convert back to scaling factor
    factors[j] = 2^(tmm)
  }

  # Normalize factors so their geometric mean is 1
  log_factors = log(factors)
  log_factors = log_factors - mean(log_factors)
  factors = exp(log_factors)

  setNames(factors, colnames(counts))
}

#' Calculate median-of-ratios normalization (DESeq2-style)
#'
#' @param counts Numeric matrix (genes x samples)
#' @return Named numeric vector of size factors (one per sample)
#' @export
calculate_median_of_ratios = function(counts) {
  nsamples = ncol(counts)

  # Compute geometric mean of each gene across all samples
  gene_means = apply(counts, 1, function(row) {
    if (any(row <= 0)) return(NA_real_)
    exp(mean(log(row)))
  })

  # Remove genes with zero geometric mean
  valid = !is.na(gene_means) & gene_means > 0
  counts_valid = counts[valid, , drop = FALSE]
  gene_means_valid = gene_means[valid]

  if (nrow(counts_valid) == 0) {
    warning("No genes with positive counts in all samples; returning unit sizes")
    return(setNames(rep(1.0, nsamples), colnames(counts)))
  }

  # For each sample, compute ratios of observed to geometric mean
  ratios = sweep(counts_valid, 1, gene_means_valid, "/")
  ratios[ratios <= 0] = NA

  # Size factor is the median of these ratios
  size_factors = apply(ratios, 2, median, na.rm = TRUE)

  # Replace NAs with 1
  size_factors[is.na(size_factors) | size_factors == 0] = 1.0

  setNames(size_factors, colnames(counts))
}

#' Normalize a count matrix using a specified method
#'
#' @param counts Numeric matrix (genes x samples)
#' @param method One of "cpm", "tmm", "median_of_ratios", or "log_tmm"
#' @return Normalized count matrix
#' @export
normalize_counts = function(counts, method = "median_of_ratios") {
  method = match.arg(method, c("cpm", "tmm", "median_of_ratios", "log_cpm"))

  switch(method,
    cpm = calculate_cpm(counts, log = FALSE),
    log_cpm = calculate_cpm(counts, log = TRUE),
    tmm = {
      factors = calculate_tmm_factors(counts)
      sweep(counts, 2, factors, "/")
    },
    median_of_ratios = {
      factors = calculate_median_of_ratios(counts)
      sweep(counts, 2, factors, "/")
    }
  )
}
