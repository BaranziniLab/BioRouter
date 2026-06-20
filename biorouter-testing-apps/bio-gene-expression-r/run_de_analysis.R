#!/usr/bin/env Rscript
# run_de_analysis.R — CLI entry point for the DE pipeline
#
# Usage:
#   Rscript run_de_analysis.R --counts counts.csv --metadata metadata.csv [options]
#
# Required arguments:
#   --counts FILE       Path to count matrix CSV/TSV (genes x samples)
#   --metadata FILE     Path to sample metadata CSV/TSV
#
# Optional arguments:
#   --sample-col COL    Column name for sample IDs (default: sample)
#   --condition-col COL Column name for condition/group (default: condition)
#   --method METHOD     DE method: quasi_likelihood, wilcoxon, t_test (default: quasi_likelihood)
#   --norm METHOD       Normalization: median_of_ratios, tmm, cpm (default: median_of_ratios)
#   --lfc THRESHOLD     Log2FC threshold for significance (default: 1.0)
#   --fdr THRESHOLD     FDR threshold for significance (default: 0.05)
#   --filter-cpm NUM    CPM threshold for low-count filtering (default: 1)
#   --output FILE       Output CSV file (default: de_results.csv)
#   --help              Show this help message

# Source all R/ modules
script_dir = getwd()
r_dir = file.path(script_dir, "R")
if (!dir.exists(r_dir)) {
  # Try relative to this script
  script_dir = dirname(sys.frame(1)$ofile %||% ".")
  r_dir = file.path(script_dir, "R")
}
for (f in list.files(r_dir, pattern = "\\.R$", full.names = TRUE)) {
  source(f)
}

# Parse command line arguments
args = commandArgs(trailingOnly = TRUE)

parse_arg = function(args, flag, default = NULL) {
  idx = which(args == flag)
  if (length(idx) == 0) return(default)
  if (idx >= length(args)) return(default)
  args[idx + 1]
}

if ("--help" %in% args || "-h" %in% args) {
  cat(readLines(file.path(script_dir, "run_de_analysis.R")), sep = "\n")
  quit(status = 0)
}

counts_file = parse_arg(args, "--counts")
metadata_file = parse_arg(args, "--metadata")
sample_col = parse_arg(args, "--sample-col", "sample")
condition_col = parse_arg(args, "--condition-col", "condition")
de_method = parse_arg(args, "--method", "quasi_likelihood")
norm_method = parse_arg(args, "--norm", "median_of_ratios")
lfc_threshold = as.numeric(parse_arg(args, "--lfc", "1.0"))
fdr_threshold = as.numeric(parse_arg(args, "--fdr", "0.05"))
filter_cpm = as.numeric(parse_arg(args, "--filter-cpm", "1"))
output_file = parse_arg(args, "--output", "de_results.csv")

if (is.null(counts_file) || is.null(metadata_file)) {
  stop("Required arguments: --counts FILE --metadata FILE\n",
       "Run with --help for usage information")
}

if (!file.exists(counts_file)) {
  stop("Count file not found: ", counts_file)
}
if (!file.exists(metadata_file)) {
  stop("Metadata file not found: ", metadata_file)
}

# Run pipeline
result = run_de_pipeline(
  counts_file = counts_file,
  metadata_file = metadata_file,
  sample_col = sample_col,
  condition_col = condition_col,
  norm_method = norm_method,
  filter_cpm = filter_cpm,
  de_method = de_method,
  lfc_threshold = lfc_threshold,
  fdr_threshold = fdr_threshold,
  output_file = output_file
)

cat(sprintf("\nAnalysis complete. Results: %s\n", output_file))
cat(sprintf("Significant genes (FDR < %.2f, |log2FC| > %.1f): %d / %d\n",
            fdr_threshold, lfc_threshold,
            result$summary$upregulated + result$summary$downregulated,
            result$summary$total_genes))
