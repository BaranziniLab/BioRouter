# ---- Tests for utils.R ----

cat("  utils.R tests\n")

test("is_binary detects binary vectors", {
  assert_true(is_binary(c(0, 1, 0, 1, 1)))
  assert_true(is_binary(c("A", "B", "A")))
  assert_false(is_binary(c(1, 2, 3)))
  assert_false(is_binary(c(1)))
})

test("binarize maps correctly", {
  # binarize sorts alphabetically: "case" < "control" so case=0, control=1
  y <- factor(c("control", "case", "case", "control"))
  b <- binarize(y)
  assert_equal(as.integer(b), c(1L, 0L, 0L, 1L))
  # With numeric labels: 10 < 20 so 10=0, 20=1
  y2 <- c(10, 20, 20, 10)
  b2 <- binarize(y2)
  assert_equal(as.integer(b2), c(0L, 1L, 1L, 0L))
})

test("row_vars and col_vars", {
  X <- matrix(1:12, nrow = 3, ncol = 4)
  rv <- row_vars(X)
  cv <- col_vars(X)
  assert_equal(length(rv), 3L)
  assert_equal(length(cv), 4L)
  # All columns have same variance (spread across 3 values)
  assert_true(all(cv > 0))
})

test("col_means", {
  X <- matrix(c(1, 2, 3, 4, 5, 6), nrow = 2, ncol = 3)
  m <- col_means(X)
  assert_equal(m, c(1.5, 3.5, 5.5))
})

test("robust_z", {
  x <- c(1, 2, 3, 4, 100)
  rz <- robust_z(x)
  assert_equal(length(rz), 5L)
  # The outlier should be z-scored highly
  assert_true(abs(rz[5]) > 2)
})

test("clip", {
  assert_equal(clip(c(-1, 0, 0.5, 1, 2), 0, 1), c(0, 0, 0.5, 1, 1))
})

test("compute_auc with perfect separation", {
  y <- c(0, 0, 0, 1, 1, 1)
  scores <- c(0.1, 0.2, 0.3, 0.8, 0.9, 1.0)
  auc <- compute_auc(y, scores)
  assert_equal(auc, 1.0)
})

test("compute_auc with random scores", {
  set.seed(42)
  y <- rep(0:1, each = 50)
  scores <- runif(100)
  auc <- compute_auc(y, scores)
  assert_gte(auc, 0.3)
  assert_lte(auc, 0.7)  # should be around 0.5
})

test("compute_accuracy", {
  y <- c(0, 0, 1, 1, 1)
  scores <- c(0.1, 0.2, 0.9, 0.8, 0.7)
  acc <- compute_accuracy(y, scores, threshold = 0.5)
  assert_equal(acc, 1.0)
})

test("kfold_indices produces correct folds", {
  folds <- kfold_indices(100, 5)
  assert_equal(length(folds), 5L)
  all_idx <- sort(unlist(folds))
  assert_equal(all_idx, 1:100)
  # Each fold has 20 elements
  assert_true(all(vapply(folds, length, integer(1)) == 20))
})

test("feature_name formatting", {
  assert_equal(feature_name("feat", 1), "feat_001")
  assert_equal(feature_name("gene", 42), "gene_042")
})

test("assess_selection", {
  truth <- c("A", "B", "C", "D")
  pred <- c("A", "B", "E")
  result <- assess_selection(pred, truth)
  assert_equal(result$overlap, 2L)
  assert_equal(result$precision, 2/3)
  assert_equal(result$recall, 0.5)
})

cat("  utils.R tests complete.\n")
