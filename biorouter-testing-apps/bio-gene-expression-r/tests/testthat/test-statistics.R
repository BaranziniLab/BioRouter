library(testthat)

test_that("estimate_dispersion returns reasonable values", {
  set.seed(42)
  # Non-DE gene: similar means across groups
  counts = c(rnbinom(4, size = 10, mu = 100), rnbinom(4, size = 10, mu = 100))
  groups = rep(c("A", "B"), each = 4)
  disp = estimate_dispersion(counts, groups)

  expect_true(is.numeric(disp))
  expect_true(disp >= 0)
  expect_true(disp < 10)
})

test_that("test_gene_qf detects DE genes", {
  set.seed(42)
  # Strong DE gene
  counts = c(rnbinom(5, size = 10, mu = 100), rnbinom(5, size = 10, mu = 800))
  groups = rep(c("ctrl", "treat"), each = 5)

  res = test_gene_qf(counts, groups)
  expect_true(!is.na(res$pvalue))
  expect_true(res$pvalue < 0.05)
  expect_true(res$log2fc > 0)
})

test_that("test_gene_qf handles non-DE genes", {
  set.seed(42)
  # Non-DE gene
  counts = c(rnbinom(5, size = 10, mu = 100), rnbinom(5, size = 10, mu = 100))
  groups = rep(c("A", "B"), each = 5)

  res = test_gene_qf(counts, groups)
  expect_true(!is.na(res$pvalue))
  expect_true(res$pvalue > 0.01)
})

test_that("differential_expression_test produces valid results", {
  set.seed(42)
  n_genes = 50
  n_samples = 8
  counts = matrix(rnbinom(n_genes * n_samples, size = 10, mu = 100),
                  nrow = n_genes, ncol = n_samples,
                  dimnames = list(paste0("G", 1:n_genes),
                                  paste0("S", 1:n_samples)))

  # Inject DE signal into first 10 genes
  counts[1:10, 5:8] = counts[1:10, 5:8] * 4

  groups = rep(c("control", "treated"), each = 4)
  results = differential_expression_test(counts, groups)

  expect_true(is.data.frame(results))
  expect_equal(nrow(results), n_genes)
  expect_true("FDR" %in% colnames(results))
  expect_true("log2FC" %in% colnames(results))

  # DE genes should rank higher (lower p-value)
  top_genes = results$gene[1:10]
  expect_true(mean(top_genes %in% paste0("G", 1:10)) > 0.5)
})

test_that("differential_expression_test works with wilcoxon method", {
  set.seed(42)
  counts = matrix(rnbinom(100, size = 10, mu = 100), nrow = 10, ncol = 8,
                  dimnames = list(paste0("G", 1:10), paste0("S", 1:8)))
  counts[1:5, 5:8] = counts[1:5, 5:8] * 5

  groups = rep(c("A", "B"), each = 4)
  results = differential_expression_test(counts, groups, method = "wilcoxon")

  expect_equal(nrow(results), 10)
  expect_true(all(!is.na(results$pvalue)))
})

test_that("differential_expression_test works with t_test method", {
  set.seed(42)
  counts = matrix(rnbinom(100, size = 10, mu = 100), nrow = 10, ncol = 8,
                  dimnames = list(paste0("G", 1:10), paste0("S", 1:8)))
  groups = rep(c("A", "B"), each = 4)
  results = differential_expression_test(counts, groups, method = "t_test")

  expect_equal(nrow(results), 10)
  expect_true(all(!is.na(results$pvalue)))
})

test_that("differential_expression_test stops with one group", {
  counts = matrix(100, nrow = 5, ncol = 4,
                  dimnames = list(paste0("G", 1:5), paste0("S", 1:4)))
  groups = rep("A", 4)

  expect_error(differential_expression_test(counts, groups),
               "at least 2 groups")
})
