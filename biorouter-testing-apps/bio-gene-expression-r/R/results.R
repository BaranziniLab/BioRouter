# results.R — Results table formatting and CSV export

#' Prepare a DE results table for CSV export
#'
#' @param results Data.frame from differential_expression_test
#' @param lfc_threshold Log2 fold-change threshold for significance
#' @param fdr_threshold FDR threshold for significance
#' @return Data.frame with additional columns: significant, regulation
#' @export
prep_for_csv = function(results, lfc_threshold = 1.0, fdr_threshold = 0.05) {
  out = results

  out$significant = out$FDR <= fdr_threshold & abs(out$log2FC) >= lfc_threshold
  out$significant[is.na(out$significant)] = FALSE

  out$regulation = "NS"
  out$regulation[out$significant & out$log2FC > 0] = "UP"
  out$regulation[out$significant & out$log2FC < 0] = "DOWN"

  # Round numeric columns for readability
  out$baseMean = round(out$baseMean, 2)
  out$log2FC = round(out$log2FC, 4)
  out$statistic = round(out$statistic, 4)
  out$pvalue = signif(out$pvalue, 6)
  out$FDR = signif(out$FDR, 6)

  # Reorder columns
  out = out[, c("gene", "baseMean", "log2FC", "statistic", "pvalue",
                "FDR", "significant", "regulation", "method")]

  out
}

#' Write DE results to a CSV file
#'
#' @param results Data.frame from prep_for_csv
#' @param file Output file path
#' @param append Whether to append to existing file
#' @return The output file path (invisibly)
#' @export
write_results_csv = function(results, file, append = FALSE) {
  dir = dirname(file)
  if (!dir.exists(dir)) {
    dir.create(dir, recursive = TRUE)
  }

  utils::write.csv(results, file = file, row.names = FALSE, quote = FALSE,
                   append = append)

  message(sprintf("Results written to %s (%d genes, %d significant)",
                  file, nrow(results),
                  sum(results$significant, na.rm = TRUE)))

  invisible(file)
}

#' Summarize DE results
#'
#' @param results Data.frame from prep_for_csv
#' @return A list with summary statistics
#' @export
summarize_results = function(results) {
  list(
    total_genes = nrow(results),
    upregulated = sum(results$regulation == "UP", na.rm = TRUE),
    downregulated = sum(results$regulation == "DOWN", na.rm = TRUE),
    not_significant = sum(results$regulation == "NS", na.rm = TRUE),
    top_gene = if (nrow(results) > 0) results$gene[1] else NA,
    min_pvalue = if (nrow(results) > 0) min(results$pvalue, na.rm = TRUE) else NA,
    min_fdr = if (nrow(results) > 0) min(results$FDR, na.rm = TRUE) else NA
  )
}

#' Print a summary of DE results to console
#'
#' @param results Data.frame from prep_for_csv
#' @return invisible NULL
#' @export
print_de_summary = function(results) {
  s = summarize_results(results)

  cat("=== Differential Expression Summary ===\n")
  cat(sprintf("  Total genes tested:     %d\n", s$total_genes))
  cat(sprintf("  Upregulated (FDR<0.05): %d\n", s$upregulated))
  cat(sprintf("  Downregulated (FDR<0.05): %d\n", s$downregulated))
  cat(sprintf("  Not significant:        %d\n", s$not_significant))
  cat(sprintf("  Top hit: %s (p=%.2e, FDR=%.2e)\n",
              s$top_gene, s$min_pvalue, s$min_fdr))
  cat("========================================\n")

  invisible(NULL)
}
