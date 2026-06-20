library(testthat)

test_that("create_volcano_data produces valid output", {
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

  expect_true(is.data.frame(volc))
  expect_true("log2FC" %in% colnames(volc))
  expect_true("negLog10FDR" %in% colnames(volc))
  expect_true("color" %in% colnames(volc))
  expect_equal(nrow(volc), 20)
  expect_equal(sum(volc$color == "UP"), 5)
  expect_equal(sum(volc$color == "DOWN"), 5)
})

test_that("create_ma_data produces valid output", {
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

  ma = create_ma_data(results)

  expect_true(is.data.frame(ma))
  expect_true("meanExpr" %in% colnames(ma))
  expect_true("log2FC" %in% colnames(ma))
  expect_true(all(ma$meanExpr >= 0))
})

test_that("plot_summary returns correct counts", {
  volc = data.frame(
    gene = paste0("G", 1:10),
    log2FC = c(rep(3, 3), rep(-3, 2), rep(0, 5)),
    pvalue = rep(0.01, 10),
    FDR = c(rep(0.01, 3), rep(0.01, 2), rep(0.5, 5)),
    negLog10FDR = runif(10, 0, 10),
    color = c(rep("UP", 3), rep("DOWN", 2), rep("NS", 5)),
    label = "",
    stringsAsFactors = FALSE
  )

  s = plot_summary(volc)
  expect_equal(s$up, 3)
  expect_equal(s$down, 2)
  expect_equal(s$ns, 5)
})
