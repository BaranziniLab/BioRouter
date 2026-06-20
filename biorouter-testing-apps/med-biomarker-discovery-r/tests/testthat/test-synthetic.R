# ---- Tests for synthetic.R ----

cat("  synthetic.R tests\n")

test("create_synthetic_data basic", {
  data <- create_synthetic_data(n_samples = 100, n_features = 50,
                                 n_informative = 5, seed = 42)
  assert_true(is.matrix(data$X))
  assert_equal(nrow(data$X), 100L)
  assert_equal(ncol(data$X), 50L)
  assert_equal(length(data$y), 100L)
  assert_equal(length(data$true_features), 5L)
  assert_true(is.numeric(data$true_coefficients))
  assert_gte(length(data$true_coefficients), 5L)
})

test("true features are actual column names", {
  data <- create_synthetic_data(n_samples = 100, n_features = 50,
                                 n_informative = 5, seed = 42)
  for (f in data$true_features) {
    assert_in(f, colnames(data$X))
  }
})

test("true features have non-zero coefficients", {
  data <- create_synthetic_data(n_samples = 100, n_features = 50,
                                 n_informative = 5, seed = 42)
  for (f in data$true_features) {
    assert_true(data$true_coefficients[f] != 0)
  }
})

test("binary outcome has only 0/1 values", {
  data <- create_synthetic_data(n_samples = 100, n_features = 50,
                                 n_informative = 5, outcome_type = "binary",
                                 seed = 42)
  unique_y <- sort(unique(data$y))
  assert_true(all(unique_y %in% c(0, 1)))
})

test("missing values are injected", {
  data <- create_synthetic_data(n_samples = 100, n_features = 50,
                                 n_informative = 5, missing_frac = 0.05,
                                 seed = 42)
  n_missing <- sum(is.na(data$X))
  assert_gte(n_missing, 1L)
})

test("generate_benchmark easy scenario", {
  data <- generate_benchmark("easy", seed = 42)
  assert_true(is.matrix(data$X))
  assert_equal(ncol(data$X), 50L)
})

test("generate_benchmark medium scenario", {
  data <- generate_benchmark("medium", seed = 42)
  assert_equal(ncol(data$X), 200L)
})

test("generate_benchmark hard scenario", {
  data <- generate_benchmark("hard", seed = 42)
  assert_equal(ncol(data$X), 500L)
})

test("get_benchmark_truth works", {
  data <- create_synthetic_data(n_samples = 100, n_features = 50,
                                 n_informative = 5, seed = 42)
  truth <- get_benchmark_truth(data)
  assert_equal(length(truth$true_features), 5L)
  assert_true(is.numeric(truth$true_coefficients))
})

test("cor_structure block works", {
  data <- create_synthetic_data(n_samples = 100, n_features = 30,
                                 n_informative = 3, cor_structure = "block",
                                 block_size = 5, seed = 42)
  assert_true(is.matrix(data$X))
  assert_equal(ncol(data$X), 30L)
})

test("cor_structure hub works", {
  data <- create_synthetic_data(n_samples = 100, n_features = 30,
                                 n_informative = 3, cor_structure = "hub",
                                 seed = 42)
  assert_true(is.matrix(data$X))
  assert_equal(ncol(data$X), 30L)
})

cat("  synthetic.R tests complete.\n")
