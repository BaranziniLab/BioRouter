#' Reporting and Summary Output
#'
#' Generate human-readable summaries of biomarker discovery results.

#' Report biomarker discovery results
#'
#' @param ranking_result List from rank_biomarker_panels.
#' @param screen_df Optional data frame from screen_univariate.
#' @param stability_result Optional list from select_features_stability.
#' @param top_n Integer. How many panels to show in detail (default 3).
#' @param file Optional file path to write the report (default NULL = stdout).
#' @return Character string of the full report (invisibly).
#' @export
report_results <- function(ranking_result, screen_df = NULL,
                           stability_result = NULL,
                           top_n = 3, file = NULL) {
  lines <- character()
  add <- function(...) lines <<- c(lines, paste0(...))

  add("=" , strrep("=", 70))
  add("  BIOMARKER DISCOVERY REPORT")
  add("=" , strrep("=", 70))
  add("")

  # --- Panel Ranking ---
  r <- ranking_result$ranking
  add("CANDIDATE PANEL RANKING (by CV AUC)")
  add(strrep("-", 70))
  add(sprintf("  %-25s %6s  %8s (%6s)  %8s (%6s)",
              "Panel", "N_feat", "AUC", "SE", "Acc", "SE"))
  add(strrep("-", 70))
  for (i in seq_len(nrow(r))) {
    add(sprintf("  %-25s %6d  %8.4f (%6.4f)  %8.4f (%6.4f)",
                r$panel[i], r$n_features[i], r$auc[i], r$auc_se[i],
                r$accuracy[i], r$accuracy_se[i]))
  }
  add(strrep("-", 70))
  add("")

  # --- Top panels detail ---
  n_show <- min(top_n, nrow(r))
  for (i in seq_len(n_show)) {
    pname <- r$panel[i]
    feats <- r$features[[i]]
    add(sprintf("PANEL %d: %s  (AUC=%.4f, Acc=%.4f, %d features)",
                i, pname, r$auc[i], r$accuracy[i], length(feats)))
    if (length(feats) <= 30) {
      add("  Features: ", paste(feats, collapse = ", "))
    } else {
      add("  Features (first 30): ", paste(head(feats, 30), collapse = ", "), "...")
    }
    add("")
  }

  # --- Effect sizes from univariate screen ---
  if (!is.null(screen_df)) {
    add("UNIVARIATE SCREENING (top 20 by p-value)")
    add(strrep("-", 70))
    top20 <- head(screen_df, min(20, nrow(screen_df)))
    for (i in seq_len(nrow(top20))) {
      bh <- if ("p_BH" %in% names(top20)) top20$p_BH[i] else NA
      add(sprintf("  %-20s  stat=%8.4f  p=%.2e  BH=%.2e  dir=%+d",
                  top20$feature[i], top20$statistic[i],
                  top20$pvalue[i],
                  ifelse(is.na(bh), NA, bh),
                  top20$direction[i]))
    }
    add("")
  }

  # --- Stability frequencies ---
  if (!is.null(stability_result)) {
    add("STABILITY SELECTION (top 20 by frequency)")
    add(strrep("-", 70))
    sf <- head(stability_result$frequency, 20)
    for (i in seq_len(nrow(sf))) {
      sel <- if (sf$selected[i]) " *" else ""
      add(sprintf("  %-20s  freq=%.3f%s",
                  sf$feature[i], sf$frequency[i], sel))
    }
    add(sprintf("  (threshold = %.2f, * = selected)", stability_result$threshold))
    add("")
  }

  add("=" , strrep("=", 70))
  add("  END OF REPORT")
  add("=" , strrep("=", 70))

  report_text <- paste(lines, collapse = "\n")
  if (!is.null(file)) {
    writeLines(report_text, file)
  }
  cat(report_text, "\n")
  invisible(report_text)
}
