# pipeline.R — End-to-end DE analysis pipeline

#' Run the complete DE analysis pipeline
#'
#' @param counts_file Path to the count matrix file
#' @param metadata_file Path to the sample metadata file
#' @param sample_col Column name for sample IDs in metadata
#' @param condition_col Column name for condition/group in metadata
#' @param norm_method Normalization method: "cpm", "tmm", "median_of_ratios"
#' @param filter_cpm CPM threshold for low-count filtering
#' @param filter_min_samples Minimum samples passing CPM threshold
#' @param de_method DE testing method: "quasi_likelihood", "wilcoxon", "t_test"
#' @param lfc_threshold Log2FC threshold for calling DE genes
#' @param fdr_threshold FDR threshold for calling DE genes
#' @param output_file Path to write results CSV
#' @return List with results, volcano_data, ma_data, pca_result, summary
#' @export
run_de_pipeline = function(counts_file,
                           metadata_file,
                           sample_col = "sample",
                           condition_col = "condition",
                           norm_method = "median_of_ratios",
                           filter_cpm = 1,
                           filter_min_samples = NULL,
                           de_method = "quasi_likelihood",
                           lfc_threshold = 1.0,
                           fdr_threshold = 0.05,
                           output_file = "de_results.csv") {
  message("=== RNA-seq Differential Expression Pipeline ===")

  # Step 1: Read data
  message("\n[1/7] Reading count matrix...")
  counts = read_count_matrix(counts_file)
  message(sprintf("  Loaded %d genes x %d samples", nrow(counts), ncol(counts)))

  message("\n[2/7] Reading sample metadata...")
  metadata = read_sample_metadata(metadata_file, sample_col, condition_col)
  aligned = align_data(counts, metadata)
  counts = aligned$counts
  metadata = aligned$metadata
  groups = metadata[[condition_col]]

  message(sprintf("  Groups: %s", paste(levels(as.factor(groups)), collapse = ", ")))

  # Step 2: Filter low counts
  message("\n[3/7] Filtering low-count genes...")
  counts_filtered = filter_low_counts(counts, cpm_threshold = filter_cpm,
                                      min_samples = filter_min_samples)
  message(sprintf("  Retained %d / %d genes",
                  nrow(counts_filtered), nrow(counts)))

  # Step 3: Normalize
  message("\n[4/7] Normalizing (", norm_method, ")...")
  counts_norm = normalize_counts(counts_filtered, method = norm_method)

  # Step 4: DE testing
  message("\n[5/7] Differential expression testing (", de_method, ")...")
  de_results = differential_expression_test(counts_norm, groups, method = de_method)

  # Step 5: Prepare results
  message("\n[6/7] Preparing results table...")
  results = prep_for_csv(de_results, lfc_threshold = lfc_threshold,
                         fdr_threshold = fdr_threshold)
  write_results_csv(results, output_file)
  print_de_summary(results)

  # Step 6: Visualization data
  volcano_data = create_volcano_data(de_results, fdr_threshold, lfc_threshold)
  ma_data = create_ma_data(de_results, fdr_threshold, lfc_threshold)

  # Step 7: PCA
  message("\n[7/7] Computing PCA...")
  pca_result = compute_pca(counts_norm)
  pca_sum = pca_summary(pca_result)
  message("  Variance explained by PC1-PC2: ",
          paste0(pca_sum$variance[1:2], "%", collapse = " / "))

  message("\n=== Pipeline complete ===")

  list(
    results = results,
    counts_raw = counts,
    counts_filtered = counts_filtered,
    counts_normalized = counts_norm,
    metadata = metadata,
    groups = groups,
    volcano_data = volcano_data,
    ma_data = ma_data,
    pca_result = pca_result,
    pca_summary = pca_sum,
    summary = summarize_results(results)
  )
}
