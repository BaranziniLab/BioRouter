#!/usr/bin/env Rscript

#' Medical Survival Analysis Script
#' 
#' Command-line interface for running survival analysis on CSV data.
#' 
#' Usage:
#'   Rscript analysis_script.R <input_csv> [--time-col TIME] [--event-col EVENT] [--group-col GROUP]
#' 
#' Arguments:
#'   input_csv         Path to CSV file with survival data
#'   --time-col        Name of time column (default: "time")
#'   --event-col       Name of event indicator column (default: "event")
#'   --group-col       Name of group column for comparison (optional)
#'   --output          Output file prefix (default: "analysis_results")
#'   --no-plot         Skip generating plots

suppressPackageStartupMessages({
  library(survival)
})

# Source the package functions
tryCatch({
  script_dir <- dirname(sys.frame(1)$ofile)
}, error = function(e) {
  script_dir <<- "."
})
if (is.null(script_dir) || script_dir == "") script_dir <- getwd()
source_files <- list.files(file.path(script_dir, "R"), pattern = "\\.R$", full.names = TRUE)
for (f in source_files) source(f)

# Parse command line arguments
args <- commandArgs(trailingOnly = TRUE)

# Default values
time_col <- "time"
event_col <- "event"
group_col <- NULL
output_prefix <- "analysis_results"
generate_plots <- TRUE
input_file <- NULL

# Parse arguments
i <- 1
while (i <= length(args)) {
  if (args[i] == "--time-col" && i < length(args)) {
    time_col <- args[i + 1]
    i <- i + 2
  } else if (args[i] == "--event-col" && i < length(args)) {
    event_col <- args[i + 1]
    i <- i + 2
  } else if (args[i] == "--group-col" && i < length(args)) {
    group_col <- args[i + 1]
    i <- i + 2
  } else if (args[i] == "--output" && i < length(args)) {
    output_prefix <- args[i + 1]
    i <- i + 2
  } else if (args[i] == "--no-plot") {
    generate_plots <- FALSE
    i <- i + 1
  } else if (args[i] %in% c("--help", "-h")) {
    cat("Medical Survival Analysis Toolkit\n\n")
    cat("Usage: Rscript analysis_script.R <input_csv> [options]\n\n")
    cat("Options:\n")
    cat("  --time-col NAME    Name of time column (default: 'time')\n")
    cat("  --event-col NAME   Name of event indicator column (default: 'event')\n")
    cat("  --group-col NAME   Name of group column for comparison\n")
    cat("  --output PREFIX    Output file prefix (default: 'analysis_results')\n")
    cat("  --no-plot          Skip generating plots\n")
    cat("  -h, --help         Show this help message\n\n")
    cat("Input CSV must contain:\n")
    cat("  - Time to event/censoring (numeric)\n")
    cat("  - Event indicator (0 = censored, 1 = event)\n")
    cat("  - Optional: grouping variable and covariates\n")
    quit(status = 0)
  } else {
    input_file <- args[i]
    i <- i + 1
  }
}

# Check input file
if (is.null(input_file)) {
  cat("Error: No input file specified\n")
  cat("Usage: Rscript analysis_script.R <input_csv> [options]\n")
  cat("Use --help for more information\n")
  quit(status = 1)
}

if (!file.exists(input_file)) {
  cat("Error: Input file not found:", input_file, "\n")
  quit(status = 1)
}

# Load data
cat("Loading data from:", input_file, "\n")
surv_data <- load_survival_data(input_file, time_col = time_col, event_col = event_col,
                                 group_col = group_col)

# Print summary
cat("\n", strrep("=", 60), "\n")
cat("SURVIVAL DATA SUMMARY\n")
cat(strrep("=", 60), "\n")
summary_stats <- summarize_survival_data(surv_data)
cat("Number of subjects:", summary_stats$n_subjects, "\n")
cat("Number of events:", summary_stats$n_events, "\n")
cat("Number censored:", summary_stats$n_censored, "\n")
cat("Event rate:", round(summary_stats$event_rate * 100, 1), "%\n")
cat("Median follow-up time:", round(summary_stats$median_time, 2), "\n")

# Kaplan-Meier estimation
cat("\n", strrep("=", 60), "\n")
cat("KAPLAN-MEIER ESTIMATION\n")
cat(strrep("=", 60), "\n")

if (!is.null(group_col) && !is.null(surv_data$group)) {
  km_result <- km_estimate(surv_data$time, surv_data$event, surv_data$group)
  
  for (g in km_result$groups) {
    cat("\nGroup:", g, "\n")
    km_g <- km_result[[g]]
    cat("  Number at risk:", km_g$n_subjects, "\n")
    cat("  Number of events:", km_g$n_total_events, "\n")
    cat("  Median survival:", round(km_g$median_survival, 2), "\n")
    cat("  95% CI: [", round(km_g$median_ci[1], 2), ",",
        round(km_g$median_ci[2], 2), "]\n")
    cat("  1-year survival:", round(km_g$survival[which(km_g$times >= 1)[1]] * 100, 1), "%\n")
    cat("  3-year survival:", round(km_g$survival[which(km_g$times >= 3)[1]] * 100, 1), "%\n")
  }
  
  # Log-rank test
  cat("\n", strrep("-", 60), "\n")
  cat("LOG-RANK TEST\n")
  cat(strrep("-", 60), "\n")
  lr_result <- log_rank_test(surv_data$time, surv_data$event, surv_data$group)
  cat("Chi-square statistic:", round(lr_result$statistic, 3), "\n")
  cat("Degrees of freedom:", lr_result$df, "\n")
  cat("P-value:", format.pval(lr_result$p_value, digits = 4), "\n")
  
  if (lr_result$p_value < 0.05) {
    cat("Conclusion: Significant difference between groups\n")
  } else {
    cat("Conclusion: No significant difference between groups\n")
  }
} else {
  km_result <- km_estimate(surv_data$time, surv_data$event)
  cat("Number at risk:", km_result$n_subjects, "\n")
  cat("Number of events:", km_result$n_total_events, "\n")
  cat("Median survival:", round(km_result$median_survival, 2), "\n")
  cat("95% CI: [", round(km_result$median_ci[1], 2), ",",
      round(km_result$median_ci[2], 2), "]\n")
}

# Cox PH regression
cat("\n", strrep("=", 60), "\n")
cat("COX PROPORTIONAL HAZARDS REGRESSION\n")
cat(strrep("=", 60), "\n")

# Prepare covariates
covariate_names <- setdiff(names(surv_data$data), c(time_col, event_col, group_col, "id", "true_time"))
if (length(covariate_names) > 0) {
  X <- surv_data$data[, covariate_names, drop = FALSE]
  # Convert factors to dummy variables
  for (col in names(X)) {
    if (is.factor(X[[col]]) || is.character(X[[col]])) {
      X[[col]] <- as.factor(X[[col]])
    }
  }
  # Create model matrix (handles factors automatically)
  X <- model.matrix(~ ., data = X)[, -1, drop = FALSE]  # Remove intercept
} else {
  X <- NULL
}

if (!is.null(X) && ncol(X) > 0) {
  cox_result <- cox_ph_model(surv_data$time, surv_data$event, X)
  
  cat("\nCoefficients:\n")
  cat("Variable            HR       95% CI              p-value\n")
  cat(strrep("-", 60), "\n")
  
  for (i in seq_along(cox_result$coefficients)) {
    var_name <- names(cox_result$coefficients)[i]
    hr <- cox_result$hazard_ratios[i]
    ci <- paste0("[", round(cox_result$ci_lower[i], 3), ", ",
                 round(cox_result$ci_upper[i], 3), "]")
    p <- format.pval(cox_result$p_value[i], digits = 4)
    cat(sprintf("%-18s %8.3f  %-20s %s\n", var_name, hr, ci, p))
  }
  
  cat("\nModel Fit:\n")
  cat("Concordance index:", round(cox_result$concordance, 3), "\n")
  cat("Log-likelihood:", round(cox_result$log_likelihood, 2), "\n")
  cat("Converged:", cox_result$converged, "\n")
  
  # PH assumption check
  cat("\n", strrep("-", 60), "\n")
  cat("PROPORTIONAL HAZARDS ASSUMPTION CHECK\n")
  cat(strrep("-", 60), "\n")
  
  ph_result <- check_ph_assumption(surv_data$time, surv_data$event, X,
                                    beta = cox_result$coefficients)
  
  cat("Overall test p-value:", format.pval(ph_result$overall_test$p_value, digits = 4), "\n\n")
  
  cat("Individual covariates:\n")
  for (i in seq_along(ph_result$p_value)) {
    var_name <- names(ph_result$p_value)[i]
    p <- format.pval(ph_result$p_value[i], digits = 4)
    rho <- round(ph_result$rho[i], 3)
    conclusion <- ph_result$conclusion[i]
    cat(sprintf("  %s: p=%s, rho=%s - %s\n", var_name, p, rho, conclusion))
  }
} else {
  cat("No covariates available for Cox PH regression\n")
  cat("(Include covariate columns in your CSV file)\n")
}

# Save results to file
output_file <- paste0(output_prefix, ".txt")
cat("\n", strrep("=", 60), "\n")
cat("Results saved to:", output_file, "\n")

# Capture output to file
sink(output_file)
cat("Medical Survival Analysis Results\n")
cat("Date:", format(Sys.time(), "%Y-%m-%d %H:%M:%S"), "\n")
cat("Input file:", input_file, "\n\n")
cat("Summary Statistics:\n")
print(summary_stats)
sink()

cat("\nAnalysis complete.\n")
