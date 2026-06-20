library(testthat)

test_that("generate_test_data creates valid data", {
  data = generate_test_data(n_genes = 100, n_samples = 6, seed = 42)

  expect_true(is.matrix(data$counts))
  expect_equal(nrow(data$counts), 100)
  expect_equal(ncol(data$counts), 6)
  expect_true(all(data$counts >= 0))

  expect_true(is.data.frame(data$metadata))
  expect_equal(nrow(data$metadata), 6)
  expect_true("condition" %in% colnames(data$metadata))
  expect_equal(length(unique(data$metadata$condition)), 2)

  expect_equal(length(data$de_gene_names), 50)
  expect_true(all(data$de_gene_names %in% rownames(data$counts)))

  # Up and down genes should be disjoint
  expect_equal(length(intersect(data$de_up_genes, data$de_down_genes)), 0)
})

test_that("write_test_data creates readable files", {
  tmp = tempdir()
  result = write_test_data(output_dir = tmp, n_genes = 50, n_samples = 4, seed = 123)

  expect_true(file.exists(result$counts_file))
  expect_true(file.exists(result$metadata_file))

  counts = read.csv(result$counts_file, row.names = 1)
  metadata = read.csv(result$metadata_file)

  expect_equal(nrow(counts), 50)
  expect_equal(nrow(metadata), 4)
  expect_true("condition" %in% colnames(metadata))
})
