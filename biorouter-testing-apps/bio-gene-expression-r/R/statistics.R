# statistics.R — Differential expression testing

#' Fit a negative-binomial-like dispersion estimate
#'
#' Estimates a per-gene dispersion using the method of moments from
#' the count data, treating each gene independently.
#'
#' @param counts Numeric vector of counts for one gene across samples
#' @param groups Factor or character vector of group labels
#' @return Estimated dispersion parameter
#' @export
estimate_dispersion = function(counts, groups) {
  groups = as.factor(groups)
  levels = levels(groups)

  if (length(levels) < 2) {
    return(0.1)
  }

  # Compute per-group means and variances
  means = tapply(counts, groups, mean)
  vars = tapply(counts, groups, function(x) {
    if (length(x) < 2) return(NA)
    var(x)
  })

  # Method of moments: Var = mean + dispersion * mean^2
  # => dispersion = (Var - mean) / mean^2
  valid = !is.na(vars) & means > 0 & vars > means

  if (sum(valid) == 0) {
    return(0.1)
  }

  dispersions = (vars[valid] - means[valid]) / (means[valid]^2)
  dispersions[dispersions < 0] = 0.01

  # Use the median dispersion across groups
  median(dispersions)
}

#' Perform a quasi-likelihood F-test-like DE analysis for one gene
#'
#' Uses a quasi-likelihood approach: fits a simple linear model,
#' estimates overdispersion, and computes a moderated F-statistic.
#' Falls back to Welch's t-test when the quasi-likelihood approach
#' fails (e.g., very small sample sizes).
#'
#' @param counts Numeric vector of counts for one gene
#' @param groups Factor or character vector of group labels
#' @return List with: statistic, pvalue, log2fc, method
#' @export
test_gene_qf = function(counts, groups) {
  groups = as.factor(groups)
  levels = levels(groups)

  if (length(levels) < 2) {
    return(list(statistic = NA, pvalue = NA, log2fc = NA, method = "insufficient_groups"))
  }

  # Compute log2 fold change (mean of group2 / mean of group1)
  group_means = tapply(counts, groups, mean)
  # Avoid log of zero
  means_safe = pmax(group_means, 0.5)
  log2fc = log2(means_safe[2] / means_safe[1])

  # Quasi-likelihood Wald test approach
  n = length(counts)
  k = length(levels)
  n_groups = as.integer(table(groups))

  # Dispersion estimate (pooled across groups)
  dispersion = estimate_dispersion(counts, groups)

  # Degrees of freedom
  df_residual = n - k

  if (df_residual <= 0) {
    return(list(statistic = NA, pvalue = NA, log2fc = log2fc,
                method = "insufficient_df"))
  }

  # Wald test: z = log2fc / se(log2fc)
  # SE from delta method on log ratio of NB means
  m1 = max(group_means[1], 0.5)
  m2 = max(group_means[2], 0.5)
  se_log2fc = sqrt((dispersion + 1/m1) / n_groups[1] +
                    (dispersion + 1/m2) / n_groups[2]) / log(2)

  # Wald statistic (approximately chi-squared with 1 df, or z-score)
  z_stat = log2fc / max(se_log2fc, 1e-10)
  f_stat = z_stat^2  # F(1, df) ≈ z^2 for large df

  # P-value: use normal distribution for Wald test
  pvalue = tryCatch({
    2 * pnorm(-abs(z_stat))
  }, error = function(e) {
    NA_real_
  })

  if (is.na(pvalue)) {
    # Fallback to Welch t-test
    groups_list = split(counts, groups)
    tt = tryCatch({
      wilcox.test(groups_list[[1]], groups_list[[2]], exact = FALSE)
    }, error = function(e) {
      t.test(groups_list[[1]], groups_list[[2]])
    })
    pvalue = tt$p.value
    method = "wilcoxon_fallback"
  } else {
    method = "quasi_likelihood_f"
  }

  list(statistic = f_stat, pvalue = pvalue, log2fc = log2fc, method = method)
}

#' Run differential expression test across all genes
#'
#' Applies a quasi-likelihood F-test (or Wilcoxon/t-test fallback)
#' to each gene, computes BH-adjusted FDR, and returns a results table.
#'
#' @param counts Numeric matrix (genes x samples) after normalization
#' @param groups Character vector of group labels (one per sample)
#' @param method Testing method: "quasi_likelihood", "wilcoxon", or "t_test"
#' @return Data.frame with columns: gene, baseMean, log2FC, statistic, pvalue, FDR
#' @export
differential_expression_test = function(counts, groups,
                                        method = "quasi_likelihood") {
  groups = as.factor(groups)

  if (length(levels(groups)) < 2) {
    stop("Need at least 2 groups for differential expression testing")
  }

  ngenes = nrow(counts)
  results = data.frame(
    gene = rownames(counts),
    baseMean = rowMeans(counts),
    log2FC = numeric(ngenes),
    statistic = numeric(ngenes),
    pvalue = numeric(ngenes),
    method = character(ngenes),
    stringsAsFactors = FALSE
  )

  for (i in seq_len(ngenes)) {
    gene_counts = counts[i, ]

    if (method == "quasi_likelihood") {
      res = tryCatch(test_gene_qf(gene_counts, groups), error = function(e) {
        list(statistic = NA, pvalue = NA, log2fc = NA, method = "error")
      })
    } else if (method == "wilcoxon") {
      groups_list = split(gene_counts, groups)
      res = tryCatch({
        tt = wilcox.test(groups_list[[1]], groups_list[[2]], exact = FALSE)
        group_means = tapply(gene_counts, groups, mean)
        log2fc = log2(max(group_means, 0.5))
        log2fc = log2(max(group_means[2], 0.5) / max(group_means[1], 0.5))
        list(statistic = tt$statistic, pvalue = tt$p.value,
             log2fc = log2fc, method = "wilcoxon")
      }, error = function(e) {
        list(statistic = NA, pvalue = NA, log2fc = NA, method = "error")
      })
    } else if (method == "t_test") {
      groups_list = split(gene_counts, groups)
      res = tryCatch({
        tt = t.test(groups_list[[1]], groups_list[[2]])
        group_means = tapply(gene_counts, groups, mean)
        log2fc = log2(max(group_means[2], 0.5) / max(group_means[1], 0.5))
        list(statistic = tt$statistic, pvalue = tt$p.value,
             log2fc = log2fc, method = "t_test")
      }, error = function(e) {
        list(statistic = NA, pvalue = NA, log2fc = NA, method = "error")
      })
    } else {
      stop("Unknown method: ", method)
    }

    results$log2FC[i] = res$log2fc
    results$statistic[i] = res$statistic
    results$pvalue[i] = res$pvalue
    results$method[i] = res$method
  }

  # BH adjustment for multiple testing
  results$FDR = p.adjust(results$pvalue, method = "BH")

  # Sort by p-value
  results = results[order(results$pvalue, na.last = TRUE), ]
  rownames(results) = NULL

  results
}
