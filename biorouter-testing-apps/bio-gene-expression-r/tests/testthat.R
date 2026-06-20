#!/usr/bin/env Rscript
# tests/testthat.R — Run all tests without package installation

# Source all R/ modules
r_dir = file.path(dirname(getwd()), "R")
if (!dir.exists(r_dir)) {
  r_dir = file.path(getwd(), "R")
}
message("Sourcing R modules from: ", r_dir)
for (f in list.files(r_dir, pattern = "\\.R$", full.names = TRUE)) {
  message("  Loading: ", basename(f))
  tryCatch(source(f), error = function(e) {
    message("    WARNING: ", conditionMessage(e))
  })
}

# Run test files directly
test_dir = file.path(getwd(), "tests", "testthat")
if (!dir.exists(test_dir)) {
  test_dir = file.path(getwd(), "testthat")
}

message("\nRunning tests from: ", test_dir)
test_files = list.files(test_dir, pattern = "^test-.*\\.R$", full.names = TRUE)
message("Found ", length(test_files), " test files")

passed = 0
failed = 0
errors = character()

for (tf in test_files) {
  message("\n--- Running: ", basename(tf), " ---")
  result = tryCatch({
    # Create a new environment for the test file
    test_env = new.env(parent = globalenv())
    # Copy all functions from the global environment to test_env
    for (n in ls(envir = .GlobalEnv)) {
      assign(n, get(n, envir = .GlobalEnv), envir = test_env)
    }
    source(tf, local = test_env)
    "PASS"
  }, error = function(e) {
    msg = conditionMessage(e)
    message("  ERROR: ", msg)
    msg
  }, warning = function(w) {
    message("  WARNING: ", conditionMessage(w))
    invokeRestart("muffleWarning")
  })

  if (identical(result, "PASS")) {
    passed = passed + 1
    message("  PASSED")
  } else {
    failed = failed + 1
    errors = c(errors, paste0(basename(tf), ": ", result))
  }
}

message("\n========================================")
message("Test Results: ", passed, " passed, ", failed, " failed")
if (length(errors) > 0) {
  message("\nFailures:")
  for (e in errors) message("  - ", e)
}
message("========================================")

quit(status = if (failed > 0) 1 else 0)
