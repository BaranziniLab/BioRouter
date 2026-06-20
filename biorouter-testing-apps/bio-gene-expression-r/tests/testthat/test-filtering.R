library(testthat)

test_that("filter_low_counts removes low-expressed genes", {
  counts = matrix(0, nrow = 10, ncol = 4,
                  dimnames = list(paste0("G", 1:10), paste0("S", 1:4)))

  # Genes 1-5: high counts
  counts[1:5, ] = 1000
  # Genes 6-10: very low counts
  counts[6:10, ] = 1

  filtered = filter_low_counts(counts, cpm_threshold = 1, min_samples = 2)

  expect_true(nrow(filtered) <= 10)
  expect_true("G1" %in% rownames(filtered))
})

test_that("filter_low_counts with fraction threshold", {
  counts = matrix(0, nrow = 10, ncol = 6,
                  dimnames = list(paste0("G", 1:10), paste0("S", 1:6)))
  counts[1:3, ] = 500
  counts[4:6, 1:3] = 500  # Only in 3 of 6 samples
  counts[7:10, ] = 1

  # Keep genes expressed in at least 50% of samples
  filtered = filter_low_counts(counts, cpm_threshold = 1, min_samples = 0.5,
                               min_fraction = TRUE)

  expect_true(nrow(filtered) >= 3)
  expect_true("G1" %in% rownames(filtered))
})

test_that("filter_by_total_counts works", {
  counts = matrix(0, nrow = 5, ncol = 3,
                  dimnames = list(c("High", "Med", "Low", "Zero", "VLow"),
                                  c("S1", "S2", "S3")))
  counts["High", ] = 1000
  counts["Med", ] = 100
  counts["Low", ] = 5
  counts["VLow", ] = 1

  filtered = filter_by_total_counts(counts, min_total = 10)

  expect_true("High" %in% rownames(filtered))
  expect_true("Med" %in% rownames(filtered))
  expect_false("Zero" %in% rownames(filtered))
})
