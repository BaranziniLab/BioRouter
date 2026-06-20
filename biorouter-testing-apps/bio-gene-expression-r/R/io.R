# io.R — Data I/O: reading count matrices and sample metadata

#' Read a count matrix from a CSV/TSV file
#'
#' Expects a file where rows are genes and columns are samples.
#' The first column contains gene identifiers.
#'
#' @param file Path to a counts file (CSV or TSV, detected by extension)
#' @return A numeric matrix with genes as rows and samples as columns;
#'         row names are gene IDs
#' @export
read_count_matrix = function(file) {
  if (!file.exists(file)) {
    stop("Count file not found: ", file)
  }

  ext = tolower(tools::file_ext(file))
  sep = if (ext == "tsv") "\t" else ","

  raw = utils::read.csv(file, header = TRUE, row.names = 1,
                        sep = sep, check.names = FALSE,
                        stringsAsFactors = FALSE)

  counts = as.matrix(raw)

  if (!is.numeric(counts)) {
    # Coerce non-numeric columns to numeric where possible
    counts = suppressWarnings(utils::type.convert(counts, as.is = TRUE))
  }

  if (anyNA(counts)) {
    stop("Count matrix contains NA values after parsing")
  }

  counts
}

#' Read sample metadata from a CSV/TSV file
#'
#' Expects a file where rows are samples and columns are variables.
#' A mandatory column named 'sample' (or 'sample_id') identifies each
#' sample; a mandatory column named 'condition' defines groups.
#'
#' @param file Path to the metadata file
#' @param sample_col Name of the sample identifier column
#' @param condition_col Name of the condition/group column
#' @return A data.frame with sample IDs as row names
#' @export
read_sample_metadata = function(file,
                                sample_col = "sample",
                                condition_col = "condition") {
  if (!file.exists(file)) {
    stop("Metadata file not found: ", file)
  }

  ext = tolower(tools::file_ext(file))
  sep = if (ext == "tsv") "\t" else ","

  meta = utils::read.csv(file, header = TRUE, sep = sep,
                         check.names = FALSE,
                         stringsAsFactors = FALSE)

  # Normalize column names: lowercase and replace spaces/hyphens with underscores
  colnames(meta) = gsub("[ -]+", "_", tolower(trimws(colnames(meta))))

  sample_col = gsub("[ -]+", "_", tolower(trimws(sample_col)))
  condition_col = gsub("[ -]+", "_", tolower(trimws(condition_col)))

  if (!(sample_col %in% colnames(meta))) {
    stop("Sample column '", sample_col, "' not found. Available: ",
         paste(colnames(meta), collapse = ", "))
  }

  if (!(condition_col %in% colnames(meta))) {
    stop("Condition column '", condition_col, "' not found. Available: ",
         paste(colnames(meta), collapse = ", "))
  }

  rownames(meta) = meta[[sample_col]]
  meta
}

#' Validate that metadata samples match count matrix columns
#'
#' @param counts Count matrix (genes x samples)
#' @param metadata Sample metadata data.frame
#' @return TRUE invisibly; stops on mismatch
#' @export
validate_metadata_match = function(counts, metadata) {
  count_samples = colnames(counts)
  meta_samples = rownames(metadata)

  missing_in_meta = setdiff(count_samples, meta_samples)
  missing_in_counts = setdiff(meta_samples, count_samples)

  if (length(missing_in_meta) > 0) {
    stop("Samples in count matrix not found in metadata: ",
         paste(missing_in_meta, collapse = ", "))
  }

  if (length(missing_in_counts) > 0) {
    warning("Samples in metadata not found in count matrix (ignored): ",
            paste(missing_in_counts, collapse = ", "))
  }

  invisible(TRUE)
}

#' Align metadata to count matrix sample order
#'
#' @param counts Count matrix
#' @param metadata Sample metadata
#' @return List with aligned `counts` and `metadata`
#' @export
align_data = function(counts, metadata) {
  common = intersect(colnames(counts), rownames(metadata))
  if (length(common) == 0) {
    stop("No common samples between count matrix and metadata")
  }
  counts = counts[, common, drop = FALSE]
  metadata = metadata[common, , drop = FALSE]
  list(counts = counts, metadata = metadata)
}
