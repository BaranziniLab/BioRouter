library(testthat)

test_that("read_count_matrix reads CSV", {
  tmp = file.path(tempdir(), "test_counts.csv")
  counts = matrix(1:12, nrow = 3, ncol = 4,
                  dimnames = list(c("G1", "G2", "G3"),
                                  c("S1", "S2", "S3", "S4")))
  utils::write.csv(counts, tmp)

  result = read_count_matrix(tmp)
  expect_true(is.matrix(result))
  expect_equal(dim(result), c(3, 4))
  expect_equal(rownames(result), c("G1", "G2", "G3"))
  expect_equal(colnames(result), c("S1", "S2", "S3", "S4"))

  unlink(tmp)
})

test_that("read_count_matrix reads TSV", {
  tmp = file.path(tempdir(), "test_counts.tsv")
  counts = matrix(1:6, nrow = 2, ncol = 3,
                  dimnames = list(c("G1", "G2"), c("S1", "S2", "S3")))
  utils::write.table(counts, tmp, sep = "\t")

  result = read_count_matrix(tmp)
  expect_equal(dim(result), c(2, 3))

  unlink(tmp)
})

test_that("read_count_matrix stops on missing file", {
  expect_error(read_count_matrix("nonexistent.csv"), "not found")
})

test_that("read_sample_metadata reads correctly", {
  tmp = file.path(tempdir(), "test_meta.csv")
  meta = data.frame(sample = c("S1", "S2", "S3"),
                    condition = c("A", "B", "A"),
                    batch = c(1, 1, 2))
  utils::write.csv(meta, tmp, row.names = FALSE)

  result = read_sample_metadata(tmp)
  expect_true(is.data.frame(result))
  expect_equal(nrow(result), 3)
  expect_true("condition" %in% colnames(result))

  unlink(tmp)
})

test_that("read_sample_metadata stops on missing column", {
  tmp = file.path(tempdir(), "test_meta2.csv")
  meta = data.frame(sample = c("S1", "S2"), batch = c(1, 2))
  utils::write.csv(meta, tmp, row.names = FALSE)

  expect_error(read_sample_metadata(tmp), "condition")

  unlink(tmp)
})

test_that("validate_metadata_match works", {
  counts = matrix(1:6, nrow = 2, ncol = 3,
                  dimnames = list(c("G1", "G2"), c("S1", "S2", "S3")))
  metadata = data.frame(sample = c("S1", "S2", "S3"),
                        condition = c("A", "B", "A"),
                        row.names = c("S1", "S2", "S3"))

  expect_true(validate_metadata_match(counts, metadata))
})

test_that("validate_metadata_match fails on mismatch", {
  counts = matrix(1:6, nrow = 2, ncol = 3,
                  dimnames = list(c("G1", "G2"), c("S1", "S2", "S3")))
  metadata = data.frame(sample = c("S1", "S2"),
                        condition = c("A", "B"),
                        row.names = c("S1", "S2"))

  expect_error(validate_metadata_match(counts, metadata))
})

test_that("align_data returns aligned objects", {
  counts = matrix(1:9, nrow = 3, ncol = 3,
                  dimnames = list(c("G1", "G2", "G3"),
                                  c("S1", "S2", "S3")))
  metadata = data.frame(sample = c("S3", "S2", "S1", "S4"),
                        condition = c("A", "B", "A", "B"),
                        row.names = c("S3", "S2", "S1", "S4"))

  result = align_data(counts, metadata)
  expect_equal(ncol(result$counts), 3)
  expect_equal(colnames(result$counts), rownames(result$metadata))
})
