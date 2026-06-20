# visualization.R — Volcano plot and MA plot data preparation

#' Create data for a volcano plot
#'
#' @param results Data.frame from differential_expression_test with columns
#'   log2FC and pvalue/FDR
#' @param fdr_threshold FDR threshold for coloring (default 0.05)
#' @param lfc_threshold Log2 fold-change threshold for coloring (default 1)
#' @return Data.frame with columns: log2FC, negLog10FDR, color
#' @export
create_volcano_data = function(results, fdr_threshold = 0.05, lfc_threshold = 1) {
  volc = data.frame(
    gene = results$gene,
    log2FC = results$log2FC,
    pvalue = results$pvalue,
    FDR = results$FDR,
    stringsAsFactors = FALSE
  )

  # -log10(FDR) for y-axis; replace NA/0 with a ceiling value
  volc$negLog10FDR = -log10(pmax(volc$FDR, 1e-300))

  # Color by significance
  volc$color = "NS"
  volc$color[volc$FDR <= fdr_threshold & volc$log2FC >= lfc_threshold] = "UP"
  volc$color[volc$FDR <= fdr_threshold & volc$log2FC <= -lfc_threshold] = "DOWN"

  # Label for top genes
  volc$label = ""
  top_genes = volc[volc$color != "NS", ]
  top_genes = top_genes[order(top_genes$pvalue), ]
  n_label = min(20, nrow(top_genes))
  if (n_label > 0) {
    volc$label[match(top_genes$gene[seq_len(n_label)], volc$gene)] =
      top_genes$gene[seq_len(n_label)]
  }

  volc
}

#' Create data for an MA plot
#'
#' @param results Data.frame from differential_expression_test
#' @param fdr_threshold FDR threshold for coloring
#' @param lfc_threshold Log2 fold-change threshold for coloring
#' @return Data.frame with columns: meanExpr, log2FC, color
#' @export
create_ma_data = function(results, fdr_threshold = 0.05, lfc_threshold = 1) {
  ma = data.frame(
    gene = results$gene,
    meanExpr = log2(pmax(results$baseMean, 1)),
    log2FC = results$log2FC,
    FDR = results$FDR,
    stringsAsFactors = FALSE
  )

  ma$color = "NS"
  ma$color[ma$FDR <= fdr_threshold & ma$log2FC >= lfc_threshold] = "UP"
  ma$color[ma$FDR <= fdr_threshold & ma$log2FC <= -lfc_threshold] = "DOWN"

  # Label top genes
  ma$label = ""
  top_genes = ma[ma$color != "NS", ]
  top_genes = top_genes[order(top_genes$FDR), ]
  n_label = min(20, nrow(top_genes))
  if (n_label > 0) {
    ma$label[match(top_genes$gene[seq_len(n_label)], ma$gene)] =
      top_genes$gene[seq_len(n_label)]
  }

  ma
}

#' Compute summary statistics for plot panels
#'
#' @param volc_data Data from create_volcano_data
#' @return List with counts and percentage info
#' @export
plot_summary = function(volc_data) {
  total = nrow(volc_data)
  list(
    total = total,
    up = sum(volc_data$color == "UP"),
    down = sum(volc_data$color == "DOWN"),
    ns = sum(volc_data$color == "NS"),
    pct_up = round(100 * sum(volc_data$color == "UP") / total, 1),
    pct_down = round(100 * sum(volc_data$color == "DOWN") / total, 1)
  )
}
