#!/usr/bin/env Rscript

#' Test Runner for medSurvivalAnalysis
#' 
#' This script runs the test suite using testthat or a simple custom harness.

suppressPackageStartupMessages({
  library(testthat)
})

# Source package functions
script_dir <- getwd()
r_dir <- file.path(script_dir, "R")
if (dir.exists(r_dir)) {
  source_files <- list.files(r_dir, pattern = "\\.R$", full.names = TRUE)
  for (f in source_files) source(f)
}

# Run tests
test_dir(file.path(script_dir, "tests", "testthat"))
