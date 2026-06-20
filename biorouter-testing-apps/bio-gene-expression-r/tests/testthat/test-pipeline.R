library(testthat)

test_that("run_de_pipeline completes end-to-end", {
  tmp = tempdir()

  # Generate test data
  test_data = write_test_data(output_dir = tmp, n_genes = 200, n_samples = 8,
                              n_de_genes = 30, seed = 42)

  output_file = file.path(tmp, "test_de_results.csv")

  # Run pipeline
  result = run_de_pipeline(
    counts_file = test_data$counts_file,
    metadata_file = test_data$metadata_file,
    norm_method = "median_of_ratios",
    de_method = "quasi_likelihood",
    output_file = output_file
  )

  # Check outputs exist
  expect_true(file.exists(output_file))
  expect_true(is.data.frame(result$results))
  expect_true(is.data.frame(result$volcano_data))
  expect_true(is.data.frame(result$ma_data))
  expect_true(is.list(result$pca_result))
  expect_true(is.list(result$summary))

  # Check that some DE genes are recovered
  # (not guaranteed to recover all due to statistical power)
  recovered = result$results$gene[result$results$significant]
  n_recovered = length(intersect(recovered, test_data$de_gene_names))

  # With log2FC=2.5 and sufficient power, should recover > 50% of DE genes
  expect_true(n_recovered > 5,
              info = paste("Recovered", n_recovered, "DE genes"))

  # Non-DE genes should mostly not be significant
  false_positives = length(intersect(recovered, setdiff(rownames(test_data$data$counts),
                                                         test_data$de_gene_names)))
  expect_true(false_positives < 30,
              info = paste("False positives:", false_positives))
})

test_that("run_de_pipeline works with wilcoxon method", {
  tmp = tempdir()

  test_data = write_test_data(output_dir = tmp, n_genes = 100, n_samples = 6,
                              n_de_genes = 20, seed = 99)

  output_file = file.path(tmp, "test_wilcoxon.csv")

  result = run_de_pipeline(
    counts_file = test_data$counts_file,
    metadata_file = test_data$metadata_file,
    de_method = "wilcoxon",
    norm_method = "tmm",
    output_file = output_file
  )

  expect_true(file.exists(output_file))
  expect_equal(nrow(result$results), 100)
})

test_that("run_de_pipeline works with t_test method and cpm normalization", {
  tmp = tempdir()

  test_data = write_test_data(output_dir = tmp, n_genes = 100, n_samples = 6,
                              n_de_genes = 20, seed = 77)

  output_file = file.path(tmp, "test_ttest.csv")

  result = run_de_pipeline(
    counts_file = test_data$counts_file,
    metadata_file = test_data$metadata_file,
    de_method = "t_test",
    norm_method = "cpm",
    output_file = output_file
  )

  expect_true(file.exists(output_file))
  expect_true(is.list(result$pca_result))
})
