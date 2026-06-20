library(testthat)
library(statSurvivalPower)

test_that("cohen_d_to_f converts correctly", {
  expect_equal(cohen_d_to_f(0.5), 0.25)
  expect_equal(cohen_d_to_f(0.0), 0.0)
  expect_equal(cohen_d_to_f(2.0), 1.0)
})

test_that("effect_size_from_cohens_f is inverse of cohen_d_to_f", {
  for (d_val in c(0.2, 0.5, 0.8, 1.2)) {
    f_val <- cohen_d_to_f(d_val)
    d_back <- effect_size_from_cohens_f(f_val)
    expect_equal(d_back, d_val, tolerance = 1e-10)
  }
})

test_that("cohen_d_to_h produces positive h for positive d", {
  h <- cohen_d_to_h(0.5)
  expect_true(h > 0)
  expect_true(is.numeric(h))
  expect_length(h, 1)
})

test_that("cohen_h_to_d is approximate inverse of cohen_d_to_h", {
  for (d_val in c(0.3, 0.5, 0.8, 1.0)) {
    h_val <- cohen_d_to_h(d_val)
    d_back <- cohen_h_to_d(h_val)
    expect_equal(d_back, d_val, tolerance = 0.01)
  }
})

test_that("cohen_h_to_w / cohen_w_to_h are inverses", {
  for (w_val in c(0.1, 0.3, 0.5)) {
    h_val <- cohen_w_to_h(w_val)
    w_back <- cohen_h_to_w(h_val)
    expect_equal(w_back, w_val, tolerance = 1e-10)
  }
})

test_that("effect_size_from_cohens_d returns expected structure", {
  es <- effect_size_from_cohens_d(0.5)
  expect_true(is.list(es))
  expect_true(all(c("eta_sq", "omega_sq", "f") %in% names(es)))
  expect_equal(es$f, cohen_d_to_f(0.5))
  expect_true(es$eta_sq >= 0 && es$eta_sq <= 1)
})
