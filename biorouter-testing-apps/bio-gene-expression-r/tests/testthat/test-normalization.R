library(testthat)

test_that("calculate_cpm produces correct values", {
  counts = matrix(c(10, 20, 30, 100, 50, 0), nrow = 2, ncol = 3,
                  dimnames = list(c("Gene1", "Gene2"),
                                  c("S1", "S2", "S3")))
  cpm = calculate_cpm(counts)

  expect_equal(dim(cpm), dim(counts))
  # CPM = count / lib_size * 1e6
  lib_sizes = colSums(counts)
  expected = sweep(counts, 2, lib_sizes / 1e6, "/")
  expect_equal(cpm, expected)
})

test_that("calculate_cpm with log = TRUE", {
  counts = matrix(c(10, 20, 30, 100), nrow = 2, ncol = 2,
                  dimnames = list(c("G1", "G2"), c("S1", "S2")))
  cpm_log = calculate_cpm(counts, log = TRUE)

  expect_true(all(cpm_log >= 0))
  # Should be log2(CPM + 1)
  cpm_raw = calculate_cpm(counts)
  expect_equal(cpm_log, log2(cpm_raw + 1))
})

test_that("calculate_tmm_factors returns unit geometric mean", {
  set.seed(42)
  counts = matrix(rnbinom(200, size = 10, mu = 100), nrow = 20, ncol = 10,
                  dimnames = list(paste0("G", 1:20), paste0("S", 1:10)))
  factors = calculate_tmm_factors(counts)

  expect_equal(length(factors), 10)
  expect_true(all(factors > 0))
  # Geometric mean of factors should be ~1
  expect_equal(exp(mean(log(factors))), 1.0, tolerance = 1e-6)
})

test_that("calculate_median_of_ratios returns reasonable size factors", {
  set.seed(42)
  # All samples have similar counts, size factors should be ~1
  counts = matrix(rnbinom(100, size = 10, mu = 100), nrow = 10, ncol = 4,
                  dimnames = list(paste0("G", 1:10), paste0("S", 1:4)))
  factors = calculate_median_of_ratios(counts)

  expect_equal(length(factors), 4)
  expect_true(all(factors > 0))
  expect_true(all(abs(factors - 1) < 0.5))
})

test_that("normalize_counts dispatches correctly", {
  counts = matrix(rnbinom(100, size = 10, mu = 100), nrow = 10, ncol = 4,
                  dimnames = list(paste0("G", 1:10), paste0("S", 1:4)))

  norm_cpm = normalize_counts(counts, method = "cpm")
  expect_equal(dim(norm_cpm), dim(counts))
  expect_true(all(norm_cpm >= 0))

  norm_tmm = normalize_counts(counts, method = "tmm")
  expect_equal(dim(norm_tmm), dim(counts))

  norm_mor = normalize_counts(counts, method = "median_of_ratios")
  expect_equal(dim(norm_mor), dim(counts))
})
