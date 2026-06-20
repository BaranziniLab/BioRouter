#!/usr/bin/env Rscript
#' =============================================================================
#' Biomarker Discovery Pipeline - Runnable Script
#'
#' Usage:
#'   Rscript run_analysis.R [--data FILE] [--outcome COL] [--output DIR]
#'                          [--lambda NUM] [--seed INT] [--demo]
#'
#' Options:
#'   --data FILE       Path to CSV (samples in rows, features in columns).
#'                     Must have an outcome column (default: "outcome").
#'   --outcome COL     Name of the outcome column (default: "outcome").
#'                     Binary (0/1) or continuous.
#'   --output DIR      Directory for output files (default: "./output").
#'   --lambda NUM      LASSO penalty (default: 0.05).
#'   --seed INT        Random seed (default: 42).
#'   --n-folds INT     CV folds (default: 5).
#'   --demo            Run with synthetic demo data (ignores --data).
#'   --help            Show this help message.
#'
#' Output:
#'   output/ranked_panels.csv     - Ranked biomarker panels with CV metrics.
#'   output/selected_features.csv - Top panel's features with effect sizes.
#'   output/report.txt            - Full text report.
#'   output/synthetic_data.csv    - (demo mode) Generated data.
#' =============================================================================

# --- Parse arguments ---
args <- commandArgs(trailingOnly = TRUE)

# Simple argument parser
parse_args <- function(args) {
  opts <- list(
    data = NULL,
    outcome = "outcome",
    output = "./output",
    lambda = 0.05,
    seed = 42,
    n_folds = 5,
    demo = FALSE
  )
  i <- 1
  while (i <= length(args)) {
    key <- args[i]
    if (key == "--demo") {
      opts$demo <- TRUE
      i <- i + 1
    } else if (key == "--help" || key == "-h") {
      cat("Usage: Rscript run_analysis.R [--data FILE] [--outcome COL] [--demo]\n")
      quit(save = "no", status = 0)
    } else if (key %in% c("--data", "--outcome", "--output")) {
      i <- i + 1
      opts[[sub("--", "", key)]] <- args[i]
      i <- i + 1
    } else if (key %in% c("--lambda", "--seed", "--n-folds")) {
      i <- i + 1
      val <- as.numeric(args[i])
      if (key == "--seed") val <- as.integer(val)
      if (key == "--n-folds") val <- as.integer(val)
      opts[[sub("--", "", key)]] <- val
      i <- i + 1
    } else {
      message("Unknown argument: ", key)
      i <- i + 1
    }
  }
  opts
}

opts <- parse_args(args)

# --- Load package ---
# Determine package root: look for DESCRIPTION in working directory or parent
find_pkg_dir <- function() {
  d <- getwd()
  while (d != dirname(d)) {
    if (file.exists(file.path(d, "DESCRIPTION"))) return(d)
    d <- dirname(d)
  }
  getwd()
}
pkg_dir <- find_pkg_dir()
# Source all R files in the package
r_files <- list.files(file.path(pkg_dir, "R"), pattern = "\\.R$", full.names = TRUE)
for (f in r_files) source(f)
message("Loaded ", length(r_files), " source files.")

# --- Ensure output directory ---
dir.create(opts$output, showWarnings = FALSE, recursive = TRUE)

# --- Load or generate data ---
if (opts$demo) {
  message("=== Generating synthetic demo data ===")
  synth <- create_synthetic_data(
    n_samples = 200, n_features = 300, n_informative = 10,
    effect_size = 1.5, cor_structure = "independent",
    missing_frac = 0.02, seed = opts$seed
  )
  X <- synth$X
  y <- synth$y
  true_features <- synth$true_features

  # Save synthetic data
  df_out <- as.data.frame(X)
  df_out$outcome <- y
  write.csv(df_out, file.path(opts$output, "synthetic_data.csv"),
            row.names = TRUE)
  message(sprintf("Synthetic data saved: %d samples x %d features + outcome",
                  nrow(X), ncol(X)))
  message("True informative features: ", paste(true_features, collapse = ", "))
} else {
  if (is.null(opts$data)) {
    cat("Error: --data FILE is required (or use --demo)\n")
    quit(save = "no", status = 1)
  }
  message("=== Loading data from ", opts$data, " ===")
  raw <- read.csv(opts$data, row.names = 1, check.names = FALSE)
  if (!(opts$outcome %in% names(raw))) {
    cat(sprintf("Error: outcome column '%s' not found. Available: %s\n",
                opts$outcome, paste(head(names(raw), 20), collapse = ", ")))
    quit(save = "no", status = 1)
  }
  y <- as.numeric(raw[[opts$outcome]])
  X <- as.matrix(raw[, setdiff(names(raw), opts$outcome)])
  message(sprintf("Loaded %d samples x %d features.", nrow(X), ncol(X)))
}

# --- Run pipeline ---
message("")
message("=== Running Biomarker Discovery Pipeline ===")
message("")

result <- pipeline(
  X, y,
  lasso_lambda = opts$lambda,
  n_cv_folds = opts$n_folds,
  seed = opts$seed,
  verbose = TRUE
)

# --- Save outputs ---
# Ranked panels
write.csv(result$ranking$ranking,
          file.path(opts$output, "ranked_panels.csv"),
          row.names = FALSE)
message("Saved: ", file.path(opts$output, "ranked_panels.csv"))

# Selected features from best panel
best_panel_name <- result$ranking$ranking$panel[1]
best_feats <- result$ranking$ranking$features[[1]]

# Compute effect sizes for selected features
screen <- result$screen
selected_effects <- screen[screen$feature %in% best_feats, ]
write.csv(selected_effects,
          file.path(opts$output, "selected_features.csv"),
          row.names = FALSE)
message("Saved: ", file.path(opts$output, "selected_features.csv"))

# Full report
writeLines(result$report,
           file.path(opts$output, "report.txt"))
message("Saved: ", file.path(opts$output, "report.txt"))

# --- Summary ---
message("")
message("=== SUMMARY ===")
message(sprintf("Best panel: %s (%d features)", best_panel_name, length(best_feats)))
message(sprintf("  CV AUC:     %.4f (SE: %.4f)",
                result$ranking$ranking$auc[1],
                result$ranking$ranking$auc_se[1]))
message(sprintf("  CV Accuracy: %.4f (SE: %.4f)",
                result$ranking$ranking$accuracy[1],
                result$ranking$ranking$accuracy_se[1]))

if (!is.null(opts$data)) {
  # If not demo, check overlap with any known true features isn't applicable
  message(sprintf("Selected features: %s", paste(best_feats, collapse = ", ")))
} else {
  overlap <- length(intersect(best_feats, true_features))
  message(sprintf("Overlap with true features: %d / %d", overlap, length(true_features)))
  message(sprintf("Recall: %.1f%%", 100 * overlap / length(true_features)))
}

message("")
message("Pipeline complete. Results in: ", opts$output)
