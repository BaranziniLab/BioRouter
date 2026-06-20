library(testthat)

test_that("prep_for_csv adds significance columns", {
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

  expect_true("significant" %in% colnames(out))
  expect_true("regulation" %in% colnames(out))
  expect_equal(sum(out$regulation == "UP"), 5)
  expect_equal(sum(out$regulation == "DOWN"), 5)
  expect_equal(sum(out$regulation == "NS"), 10)
})

test_that("write_results_csv creates a file", {
  results = data.frame(
    gene = "G1", baseMean = 100, log2FC = 2, statistic = 5,
    pvalue = 0.01, FDR = 0.05, significant = TRUE,
    regulation = "UP", method = "test",
    stringsAsFactors = FALSE
  )

  tmp = file.path(tempdir(), "test_results.csv")
  write_results_csv(results, tmp)

  expect_true(file.exists(tmp))
  written = read.csv(tmp)
  expect_equal(nrow(written), 1)
  expect_true("regulation" %in% colnames(written))

  # Clean up
  unlink(tmp)
})

test_that("summarize_results returns correct counts", {
  results = data.frame(
    gene = paste0("G", 1:10),
    baseMean = rep(100, 10),
    log2FC = c(rep(2, 3), rep(-2, 2), rep(0, 5)),
    statistic = rep(5, 10),
    pvalue = c(rep(0.001, 3), rep(0.001, 2), rep(0.5, 5)),
    FDR = c(rep(0.01, 3), rep(0.01, 2), rep(0.9, 5)),
    significant = c(rep(TRUE, 5), rep(FALSE, 5)),
    regulation = c(rep("UP", 3), rep("DOWN", 2), rep("NS", 5)),
    method = "test",
    stringsAsFactors = FALSE
  )

  s = summarize_results(results)
  expect_equal(s$total_genes, 10)
  expect_equal(s$upregulated, 3)
  expect_equal(s$downregulated, 2)
})
