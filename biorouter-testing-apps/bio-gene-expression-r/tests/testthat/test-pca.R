library(testthat)

test_that("compute_pca returns valid results", {
  set.seed(42)
  counts = matrix(rnbinom(200, size = 10, mu = 100), nrow = 20, ncol = 10,
                  dimnames = list(paste0("G", 1:20), paste0("S", 1:10)))

  pca = compute_pca(counts)

  expect_true(is.data.frame(pca$coordinates))
  expect_equal(nrow(pca$coordinates), 10)
  expect_true(all(grepl("^PC", colnames(pca$coordinates))))

  expect_true(is.numeric(pca$var_explained))
  expect_equal(length(pca$var_explained), 10)
  # Variance explained should sum to <= 1
  expect_true(sum(pca$var_explained) <= 1.0 + 1e-10)

  expect_true(is.data.frame(pca$loadings))
  expect_equal(nrow(pca$loadings), 20)
})

test_that("pca_summary returns correct format", {
  set.seed(42)
  counts = matrix(rnbinom(200, size = 10, mu = 100), nrow = 20, ncol = 10,
                  dimnames = list(paste0("G", 1:20), paste0("S", 1:10)))

  pca = compute_pca(counts)
  s = pca_summary(pca, n_components = 3)

  expect_equal(nrow(s), 3)
  expect_true("component" %in% colnames(s))
  expect_true("variance" %in% colnames(s))
  expect_true("cumulative" %in% colnames(s))
  expect_true(s$cumulative[3] <= 100)
})

test_that("pca separates distinct groups", {
  set.seed(42)
  n_genes = 50
  n_per_group = 5

  # Group A: high counts
  counts_a = matrix(rnbinom(n_genes * n_per_group, size = 10, mu = 200),
                    nrow = n_genes, ncol = n_per_group)
  # Group B: low counts
  counts_b = matrix(rnbinom(n_genes * n_per_group, size = 10, mu = 50),
                    nrow = n_genes, ncol = n_per_group)

  counts = cbind(counts_a, counts_b)
  colnames(counts) = paste0("S", 1:10)
  rownames(counts) = paste0("G", 1:n_genes)

  pca = compute_pca(counts)

  # PC1 should separate the two groups
  pc1 = pca$coordinates$PC1
  group_a_pc1 = mean(pc1[1:5])
  group_b_pc1 = mean(pc1[6:10])

  # Groups should be separated on PC1
  expect_true(abs(group_a_pc1 - group_b_pc1) > 0.1)
})
