#!/usr/bin/env Rscript
#' Simple test harness for biomarkerDiscovR (no testthat dependency required).
#'
#' Usage: Rscript tests/run_tests.R

cat("========================================\n")
cat("  biomarkerDiscovR Test Suite\n")
cat("========================================\n\n")

# --- Load all source files ---
pkg_dir <- getwd()
r_files <- list.files(file.path(pkg_dir, "R"), pattern = "\\.R$", full.names = TRUE)
for (f in r_files) {
  tryCatch(source(f), error = function(e) {
    cat(sprintf("FAIL loading %s: %s\n", basename(f), e$message))
  })
}
cat(sprintf("Loaded %d source files.\n\n", length(r_files)))

# --- Test framework ---
n_pass <- 0L
n_fail <- 0L
n_skip <- 0L
failures <- character()

test <- function(name, expr) {
  result <- tryCatch(
    { expr; TRUE },
    error = function(e) e$message
  )
  if (isTRUE(result)) {
    n_pass <<- n_pass + 1L
    cat(sprintf("  PASS  %s\n", name))
  } else if (is.character(result) && grepl("^SKIP:", result)) {
    n_skip <<- n_skip + 1L
    cat(sprintf("  SKIP  %s (%s)\n", name, sub("^SKIP: ", "", result)))
  } else {
    n_fail <<- n_fail + 1L
    msg <- as.character(result)
    cat(sprintf("  FAIL  %s\n        %s\n", name, msg))
    failures <<- c(failures, sprintf("%s: %s", name, msg))
  }
}

assert <- function(condition, msg = "assertion failed") {
  if (!isTRUE(condition)) stop(msg, call. = FALSE)
}

assert_true <- function(x, msg = "expected TRUE") {
  if (!isTRUE(x)) stop(msg, call. = FALSE)
}

assert_false <- function(x, msg = "expected FALSE") {
  if (!isFALSE(x)) stop(msg, call. = FALSE)
}

assert_equal <- function(a, b, msg = NULL) {
  if (!isTRUE(all.equal(a, b, check.attributes = FALSE))) {
    if (is.null(msg)) msg <- sprintf("expected %s, got %s", deparse(b), deparse(a))
    stop(msg, call. = FALSE)
  }
}

assert_true_fn <- function(x) assert_true(isTRUE(x) || isTRUE(x > 0), "expected truthy")

assert_gte <- function(a, b, msg = NULL) {
  if (a < b) {
    if (is.null(msg)) msg <- sprintf("expected %s >= %s", a, b)
    stop(msg, call. = FALSE)
  }
}

assert_lte <- function(a, b, msg = NULL) {
  if (a > b) {
    if (is.null(msg)) msg <- sprintf("expected %s <= %s", a, b)
    stop(msg, call. = FALSE)
  }
}

assert_in <- function(x, table, msg = NULL) {
  if (!(x %in% table)) {
    if (is.null(msg)) msg <- sprintf("%s not found in expected set", deparse(x))
    stop(msg, call. = FALSE)
  }
}

assert_error <- function(expr, msg = NULL) {
  result <- tryCatch(expr, error = function(e) e$message)
  if (!is.character(result) || length(result) == 0) {
    if (is.null(msg)) msg <- "expected an error but none was raised"
    stop(msg, call. = FALSE)
  }
}

# ---- Source test files ----
test_files <- list.files(file.path(pkg_dir, "tests", "testthat"),
                         pattern = "^test-.*\\.R$", full.names = TRUE)
for (tf in test_files) {
  cat(sprintf("\n--- %s ---\n", basename(tf)))
  tryCatch(source(tf), error = function(e) {
    cat(sprintf("  ERROR loading test file: %s\n", e$message))
    n_fail <<- n_fail + 1L
  })
}

# ---- Summary ----
cat("\n========================================\n")
cat(sprintf("  Results: %d passed, %d failed, %d skipped\n", n_pass, n_fail, n_skip))
cat("========================================\n")

if (n_fail > 0) {
  cat("\nFailures:\n")
  for (f in failures) cat(sprintf("  - %s\n", f))
  quit(save = "no", status = 1)
} else {
  cat("\nAll tests passed!\n")
  quit(save = "no", status = 0)
}
