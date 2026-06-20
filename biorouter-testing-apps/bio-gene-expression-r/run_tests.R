#!/usr/bin/env Rscript
# run_tests.R — Standalone test runner (no testthat dependency)

r_dir = "R"
if (!dir.exists(r_dir)) r_dir = "."
message("Sourcing modules from: ", r_dir)
for (f in list.files(r_dir, pattern = "\\.R$", full.names = TRUE)) {
  source(f, local = globalenv())
}

test_count = 0L
pass_count = 0L
fail_count = 0L
failures = character()

assert_true = function(expr, label = "") {
  test_count <<- test_count + 1L
  result = tryCatch(as.logical(expr), error = function(e) FALSE)
  if (isTRUE(result)) {
    pass_count <<- pass_count + 1L
  } else {
    fail_count <<- fail_count + 1L
    msg = sprintf("FAIL: %s", label)
    failures <<- c(failures, msg)
    message("  ", msg)
  }
}

assert_equal = function(actual, expected, label = "", tolerance = NULL) {
  test_count <<- test_count + 1L
  if (is.null(tolerance)) {
    result = isTRUE(all.equal(actual, expected, check.attributes = FALSE))
  } else {
    result = isTRUE(all.equal(actual, expected, tolerance = tolerance))
  }
  if (result) {
    pass_count <<- pass_count + 1L
  } else {
    fail_count <<- fail_count + 1L
    msg = sprintf("FAIL: %s", label)
    failures <<- c(failures, msg)
    message("  ", msg)
  }
}

assert_error = function(expr, label = "") {
  test_count <<- test_count + 1L
  result = tryCatch({ eval(expr); FALSE }, error = function(e) TRUE)
  if (result) {
    pass_count <<- pass_count + 1L
  } else {
    fail_count <<- fail_count + 1L
    msg = sprintf("FAIL: %s (expected error)", label)
    failures <<- c(failures, msg)
    message("  ", msg)
  }
}

assert_range = function(x, lower, upper, label = "") {
  test_count <<- test_count + 1L
  if (all(x >= lower) && all(x <= upper)) {
    pass_count <<- pass_count + 1L
  } else {
    fail_count <<- fail_count + 1L
    msg = sprintf("FAIL: %s", label)
    failures <<- c(failures, msg)
    message("  ", msg)
  }
}

assert_false = function(expr, label = "") {
  test_count <<- test_count + 1L
  result = tryCatch(as.logical(expr), error = function(e) TRUE)
  if (isFALSE(result)) {
    pass_count <<- pass_count + 1L
  } else {
    fail_count <<- fail_count + 1L
    msg = sprintf("FAIL: %s", label)
    failures <<- c(failures, msg)
    message("  ", msg)
  }
}

run_section = function(name, expr) {
  message("\n=== ", name, " ===")
  tryCatch(expr, error = function(e) {
    fail_count <<- fail_count + 1L
    msg = sprintf("ERROR in %s: %s", name, conditionMessage(e))
    failures <<- c(failures, msg)
    message("  ", msg)
  })
}

# ============================================================
# TESTS
# ============================================================

run_section("Synthetic Data Generation", {
  data = generate_test_data(n_genes = 100, n_samples = 6, seed = 42)
  assert_true(is.matrix(data$counts), "counts is matrix")
  assert_equal(nrow(data$counts), 100, "100 genes")
  assert_equal(ncol(data$counts), 6, "6 samples")
  assert_true(all(data$counts >= 0), "counts non-negative")
  assert_true(is.data.frame(data$metadata), "metadata is data.frame")
  assert_equal(nrow(data$metadata), 6, "6 metadata rows")
  assert_equal(length(data$de_gene_names), 50, "50 DE genes")
  assert_equal(length(intersect(data$de_up_genes, data$de_down_genes)), 0,
               "DE up/down disjoint")
})

run_section("I/O Functions", {
  counts = matrix(1:12, nrow = 3, ncol = 4,
                  dimnames = list(c("G1", "G2", "G3"), c("S1", "S2", "S3", "S4")))
  tmp_csv = file.path(tempdir(), "io_test.csv")
  utils::write.csv(counts, tmp_csv)
  loaded = read_count_matrix(tmp_csv)
  assert_equal(dim(loaded), c(3, 4), "loaded dimensions")
  assert_equal(rownames(loaded), c("G1", "G2", "G3"), "gene names preserved")
  unlink(tmp_csv)

  meta = data.frame(sample = c("S1", "S2"), condition = c("A", "B"),
                    row.names = c("S1", "S2"))
  tmp_meta = file.path(tempdir(), "meta_test.csv")
  utils::write.csv(meta, tmp_meta, row.names = FALSE)
  loaded_meta = read_sample_metadata(tmp_meta)
  assert_equal(nrow(loaded_meta), 2, "meta rows")
  assert_true("condition" %in% colnames(loaded_meta), "condition column exists")
  unlink(tmp_meta)

  assert_error(quote(read_count_matrix("nonexistent.csv")), "missing file error")
})

run_section("CPM Normalization", {
  counts = matrix(c(100, 200, 300, 400), nrow = 2, ncol = 2,
                  dimnames = list(c("G1", "G2"), c("S1", "S2")))
  cpm = calculate_cpm(counts)
  assert_equal(dim(cpm), dim(counts), "CPM dimensions")
  assert_range(cpm[1, 1], 333333, 333334, "CPM G1/S1")
  assert_true(all(cpm >= 0), "CPM non-negative")

  cpm_log = calculate_cpm(counts, log = TRUE)
  assert_true(all(cpm_log >= 0), "log CPM non-negative")
  assert_true(all(is.finite(cpm_log)), "log CPM finite")
})

run_section("TMM Factors", {
  set.seed(42)
  counts = matrix(rnbinom(200, size = 10, mu = 100), nrow = 20, ncol = 5,
                  dimnames = list(paste0("G", 1:20), paste0("S", 1:5)))
  factors = calculate_tmm_factors(counts)
  assert_equal(length(factors), 5, "5 factors")
  assert_true(all(factors > 0), "factors positive")
  assert_equal(exp(mean(log(factors))), 1.0, "geometric mean = 1", tolerance = 1e-6)
})

run_section("Median of Ratios", {
  set.seed(42)
  counts = matrix(rnbinom(100, size = 10, mu = 100), nrow = 10, ncol = 4,
                  dimnames = list(paste0("G", 1:10), paste0("S", 1:4)))
  factors = calculate_median_of_ratios(counts)
  assert_equal(length(factors), 4, "4 factors")
  assert_true(all(factors > 0), "factors positive")
  assert_range(factors, 0.5, 2.0, "factors near 1")
})

run_section("Low-Count Filtering", {
  counts = matrix(0, nrow = 10, ncol = 4,
                  dimnames = list(paste0("G", 1:10), paste0("S", 1:4)))
  counts[1:5, ] = 1000
  counts[6:10, ] = 0
  filtered = filter_low_counts(counts, cpm_threshold = 100, min_samples = 2)
  assert_true(nrow(filtered) <= 10, "filtered <= original")
  assert_true("G1" %in% rownames(filtered), "high-count gene kept")
  assert_false("G6" %in% rownames(filtered), "low-count gene removed")
})

run_section("DE Testing - Quasi-Likelihood", {
  set.seed(42)
  counts = c(rnbinom(5, size = 10, mu = 100), rnbinom(5, size = 10, mu = 800))
  groups = rep(c("ctrl", "treat"), each = 5)
  res = test_gene_qf(counts, groups)
  assert_true(!is.na(res$pvalue), "pvalue not NA")
  assert_true(res$pvalue < 0.05, "DE gene significant")
  assert_true(res$log2fc > 0, "DE gene positive log2FC")
})

run_section("DE Testing - Full Pipeline", {
  set.seed(42)
  n_genes = 50
  n_samples = 8
  counts = matrix(rnbinom(n_genes * n_samples, size = 10, mu = 100),
                  nrow = n_genes, ncol = n_samples,
                  dimnames = list(paste0("G", 1:n_genes), paste0("S", 1:n_samples)))
  counts[1:10, 5:8] = counts[1:10, 5:8] * 4
  groups = rep(c("control", "treated"), each = 4)
  results = differential_expression_test(counts, groups)
  assert_true(is.data.frame(results), "results is data.frame")
  assert_equal(nrow(results), n_genes, "all genes tested")
  assert_true("FDR" %in% colnames(results), "FDR column")
  assert_true("log2FC" %in% colnames(results), "log2FC column")
  top_genes = results$gene[1:10]
  assert_true(mean(top_genes %in% paste0("G", 1:10)) > 0.5, "DE genes rank higher")
})

run_section("DE Testing - Wilcoxon", {
  set.seed(42)
  counts = matrix(0, nrow = 10, ncol = 8,
                  dimnames = list(paste0("G", 1:10), paste0("S", 1:8)))
  for (i in 1:10) counts[i, ] = rnbinom(8, size = 10, mu = 100)
  counts[1:5, 5:8] = counts[1:5, 5:8] * 5
  groups = rep(c("A", "B"), each = 4)
  results = differential_expression_test(counts, groups, method = "wilcoxon")
  assert_equal(nrow(results), 10, "10 genes")
  assert_true(all(!is.na(results$pvalue)), "no NA pvalues")
})

run_section("Results Formatting", {
  results = data.frame(
    gene = paste0("G", 1:20),
    baseMean = runif(20, 50, 200),
    log2FC = c(rep(3, 5), rep(-3, 5), rep(0.2, 10)),
    statistic = runif(20, 1, 10),
    pvalue = c(rep(0.001, 5), rep(0.001, 5), rep(0.5, 10)),
    FDR = c(rep(0.01, 5), rep(0.01, 5), rep(0.9, 10)),
    method = "test",
    stringsAsFactors = FALSE
  )
  out = prep_for_csv(results)
  assert_true("significant" %in% colnames(out), "significant column")
  assert_true("regulation" %in% colnames(out), "regulation column")
  assert_equal(sum(out$regulation == "UP"), 5, "5 upregulated")
  assert_equal(sum(out$regulation == "DOWN"), 5, "5 downregulated")
  assert_equal(sum(out$regulation == "NS"), 10, "10 NS")

  tmp = file.path(tempdir(), "results_test.csv")
  write_results_csv(out, tmp)
  assert_true(file.exists(tmp), "CSV written")
  written = read.csv(tmp)
  assert_equal(nrow(written), 20, "CSV rows")
  unlink(tmp)
})

run_section("Volcano & MA Data", {
  results = data.frame(
    gene = paste0("G", 1:20),
    baseMean = runif(20, 50, 200),
    log2FC = c(rep(3, 5), rep(-3, 5), runif(10, -0.5, 0.5)),
    statistic = runif(20, 1, 10),
    pvalue = c(rep(1e-6, 5), rep(1e-6, 5), runif(10, 0.1, 0.9)),
    FDR = c(rep(1e-4, 5), rep(1e-4, 5), runif(10, 0.3, 1)),
    method = "test",
    stringsAsFactors = FALSE
  )
  volc = create_volcano_data(results)
  assert_true(is.data.frame(volc), "volcano is data.frame")
  assert_true("negLog10FDR" %in% colnames(volc), "negLog10FDR column")
  assert_equal(sum(volc$color == "UP"), 5, "5 UP in volcano")
  assert_equal(sum(volc$color == "DOWN"), 5, "5 DOWN in volcano")

  ma = create_ma_data(results)
  assert_true(is.data.frame(ma), "MA is data.frame")
  assert_true("meanExpr" %in% colnames(ma), "meanExpr column")
  assert_true(all(ma$meanExpr >= 0), "MA meanExpr non-negative")
})

run_section("PCA", {
  set.seed(42)
  counts = matrix(rnbinom(200, size = 10, mu = 100), nrow = 20, ncol = 10,
                  dimnames = list(paste0("G", 1:20), paste0("S", 1:10)))
  pca = compute_pca(counts)
  assert_true(is.data.frame(pca$coordinates), "coords is data.frame")
  assert_equal(nrow(pca$coordinates), 10, "10 samples in PCA")
  assert_true(is.numeric(pca$var_explained), "var_explained numeric")
  assert_equal(length(pca$var_explained), 10, "10 components")
  s = pca_summary(pca, n_components = 3)
  assert_equal(nrow(s), 3, "3-component summary")
  assert_true("variance" %in% colnames(s), "variance column")
})

run_section("PCA Group Separation", {
  set.seed(42)
  n_genes = 50
  counts_a = matrix(rnbinom(n_genes * 5, size = 10, mu = 200), nrow = n_genes, ncol = 5)
  counts_b = matrix(rnbinom(n_genes * 5, size = 10, mu = 50), nrow = n_genes, ncol = 5)
  counts = cbind(counts_a, counts_b)
  colnames(counts) = paste0("S", 1:10)
  rownames(counts) = paste0("G", 1:n_genes)
  pca = compute_pca(counts)
  pc1 = pca$coordinates$PC1
  assert_true(abs(mean(pc1[1:5]) - mean(pc1[6:10])) > 0.1, "PC1 separates groups")
})

run_section("Full Pipeline Integration", {
  tmp = tempdir()
  test_data = write_test_data(output_dir = tmp, n_genes = 200, n_samples = 8,
                              n_de_genes = 30, seed = 42)
  output_file = file.path(tmp, "pipeline_test_results.csv")
  result = run_de_pipeline(
    counts_file = test_data$counts_file,
    metadata_file = test_data$metadata_file,
    norm_method = "median_of_ratios",
    de_method = "quasi_likelihood",
    output_file = output_file
  )
  assert_true(file.exists(output_file), "output CSV exists")
  assert_true(is.data.frame(result$results), "results is data.frame")
  assert_true(is.data.frame(result$volcano_data), "volcano data exists")
  assert_true(is.data.frame(result$ma_data), "MA data exists")
  assert_true(is.list(result$pca_result), "PCA result exists")
  assert_true(is.list(result$summary), "summary exists")
  recovered = result$results$gene[result$results$significant]
  n_recovered = length(intersect(recovered, test_data$de_gene_names))
  message(sprintf("  Recovered %d / %d known DE genes", n_recovered, 30))
  assert_true(n_recovered > 5, "recovered > 5 DE genes")
})

run_section("Pipeline with Wilcoxon + TMM", {
  tmp = tempdir()
  test_data = write_test_data(output_dir = tmp, n_genes = 100, n_samples = 6,
                              n_de_genes = 20, seed = 99)
  output_file = file.path(tmp, "wilcoxon_test.csv")
  result = run_de_pipeline(
    counts_file = test_data$counts_file,
    metadata_file = test_data$metadata_file,
    de_method = "wilcoxon",
    norm_method = "tmm",
    output_file = output_file
  )
  assert_true(file.exists(output_file), "wilcoxon output exists")
  assert_equal(nrow(result$results), 100, "100 genes in wilcoxon results")
})

# ============================================================
# SUMMARY
# ============================================================
message("\n========================================")
message(sprintf("Test Results: %d passed, %d failed (out of %d)",
                pass_count, fail_count, test_count))
if (fail_count > 0) {
  message("\nFailures:")
  for (f in failures) message("  - ", f)
}
message("========================================")

quit(status = if (fail_count > 0) 1 else 0)
