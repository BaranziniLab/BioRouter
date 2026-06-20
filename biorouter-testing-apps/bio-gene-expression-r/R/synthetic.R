# synthetic.R — Generate synthetic test data with known DE genes

#' Generate synthetic RNA-seq count data with known differential expression
#'
#' Creates a count matrix and metadata file for testing the DE pipeline.
#' Some genes are injected with known fold-changes between conditions.
#'
#' @param n_genes Total number of genes to simulate
#' @param n_samples Number of samples (split evenly between conditions)
#' @param n_de_genes Number of differentially expressed genes (half up, half down)
#' @param base_mean Mean expression level for non-DE genes
#' @param de_log2fc Log2 fold-change for DE genes
#' @param dispersion Overdispersion parameter
#' @param seed Random seed for reproducibility
#' @return List with counts (matrix), metadata (data.frame), de_gene_names (character vector)
#' @export
generate_test_data = function(n_genes = 1000,
                              n_samples = 8,
                              n_de_genes = 50,
                              base_mean = 100,
                              de_log2fc = 2.5,
                              dispersion = 0.5,
                              seed = 42) {
  set.seed(seed)

  n_de_genes = min(n_de_genes, n_genes)
  n_de_up = floor(n_de_genes / 2)
  n_de_down = n_de_genes - n_de_up

  # Gene names
  all_genes = paste0("Gene", seq_len(n_genes))

  # DE gene indices
  de_up_idx = seq_len(n_de_up)
  de_down_idx = seq_len(n_de_down) + n_de_up
  de_gene_names = all_genes[c(de_up_idx, de_down_idx)]

  # Conditions
  conditions = rep(c("control", "treated"), each = n_samples / 2)
  sample_names = paste0("Sample", seq_len(n_samples))

  # Generate counts using negative binomial
  counts = matrix(0, nrow = n_genes, ncol = n_samples,
                  dimnames = list(all_genes, sample_names))

  for (i in seq_len(n_genes)) {
    for (j in seq_len(n_samples)) {
      mu = base_mean

      # Apply fold change for DE genes
      if (i %in% de_up_idx && conditions[j] == "treated") {
        mu = mu * 2^de_log2fc
      } else if (i %in% de_down_idx && conditions[j] == "treated") {
        mu = mu * 2^(-de_log2fc)
      }

      # Add some per-sample variability (library size differences)
      lib_factor = rlnorm(1, meanlog = 0, sdlog = 0.1)
      mu = mu * lib_factor

      # Negative binomial sampling
      size = 1 / dispersion  # RB parameterization
      counts[i, j] = rnbinom(1, size = size, mu = mu)
    }
  }

  # Metadata
  metadata = data.frame(
    sample = sample_names,
    condition = conditions,
    stringsAsFactors = FALSE
  )

  list(
    counts = counts,
    metadata = metadata,
    de_gene_names = de_gene_names,
    de_up_genes = all_genes[de_up_idx],
    de_down_genes = all_genes[de_down_idx],
    params = list(
      n_genes = n_genes,
      n_samples = n_samples,
      n_de_genes = n_de_genes,
      base_mean = base_mean,
      de_log2fc = de_log2fc,
      dispersion = dispersion,
      seed = seed
    )
  )
}

#' Write synthetic test data to files
#'
#' @param output_dir Directory to write files into
#' @param ... Arguments passed to generate_test_data
#' @return List with file paths and ground truth
#' @export
write_test_data = function(output_dir = tempdir(), ...) {
  data = generate_test_data(...)

  counts_file = file.path(output_dir, "test_counts.csv")
  metadata_file = file.path(output_dir, "test_metadata.csv")

  # Write counts
  utils::write.csv(data$counts, counts_file)

  # Write metadata
  utils::write.csv(data$metadata, metadata_file, row.names = FALSE)

  list(
    counts_file = counts_file,
    metadata_file = metadata_file,
    de_gene_names = data$de_gene_names,
    de_up_genes = data$de_up_genes,
    de_down_genes = data$de_down_genes,
    data = data
  )
}
