#' Synthetic Data Generation for Benchmarking
#'
#' Generate high-dimensional datasets with known informative features.

#' Create synthetic biomarker data
#'
#' @param n_samples Integer. Number of samples (default 200).
#' @param n_features Integer. Total number of features (default 500).
#' @param n_informative Integer. Number of truly informative features (default 15).
#' @param n_noise Integer. Number of pure-noise features (default 0; remainder after informative).
#' @param outcome_type Character. "binary" or "continuous" (default "binary").
#' @param effect_size Numeric. Magnitude of informative features' effect (default 1.5).
#' @param noise_sd Numeric. Standard deviation of noise (default 1.0).
#' @param cor_structure Character. "independent", "block", or "hub" (default "independent").
#' @param block_size Integer. For "block" correlation: size of correlated blocks (default 10).
#' @param misssing_frac Numeric. Fraction of entries to set NA (default 0.02).
#' @param seed Integer. Random seed (default 42).
#' @return List with:
#'   \item{X}{n_samples x n_features numeric matrix.}
#'   \item{y}{Outcome vector (0/1 for binary).}
#'   \item{true_features}{Character vector of truly informative feature names.}
#'   \item{true_coefficients}{Named numeric vector of true coefficients (non-zero only).}
#'   \item{metadata}{List of generation parameters.}
#' @export
create_synthetic_data <- function(n_samples = 200,
                                   n_features = 500,
                                   n_informative = 15,
                                   n_noise = NULL,
                                   outcome_type = c("binary", "continuous"),
                                   effect_size = 1.5,
                                   noise_sd = 1.0,
                                   cor_structure = c("independent", "block", "hub"),
                                   block_size = 10,
                                   missing_frac = 0.02,
                                   seed = 42) {
  outcome_type <- match.arg(outcome_type)
  cor_structure <- match.arg(cor_structure)
  set.seed(seed)

  if (is.null(n_noise)) {
    n_noise <- n_features - n_informative
  }

  feat_names <- paste0("feat_", seq_len(n_features))
  true_names <- paste0("feat_", seq_len(n_informative))

  # --- Generate correlated feature matrix ---
  X <- matrix(NA_real_, nrow = n_samples, ncol = n_features,
              dimnames = list(paste0("sample_", seq_len(n_samples)), feat_names))

  if (cor_structure == "independent") {
    X <- matrix(rnorm(n_samples * n_features, sd = noise_sd),
                nrow = n_samples, ncol = n_features,
                dimnames = list(paste0("sample_", seq_len(n_samples)), feat_names))
  } else if (cor_structure == "block") {
    # Independent blocks with intra-block correlation
    rho <- 0.6
    n_blocks <- ceiling(n_features / block_size)
    for (b in seq_len(n_blocks)) {
      start_col <- (b - 1) * block_size + 1
      end_col <- min(b * block_size, n_features)
      n_in_block <- end_col - start_col + 1
      # Generate shared signal + independent noise
      shared <- rnorm(n_samples)
      for (j in seq_len(n_in_block)) {
        X[, start_col + j - 1] <- sqrt(rho) * shared + sqrt(1 - rho) * rnorm(n_samples, sd = noise_sd)
      }
    }
  } else {
    # Hub: first few features are hubs
    n_hubs <- min(5, n_informative)
    hub_signals <- matrix(rnorm(n_samples * n_hubs), nrow = n_samples, ncol = n_hubs)
    for (j in seq_len(n_features)) {
      hub_idx <- ((j - 1) %% n_hubs) + 1
      rho <- 0.4
      X[, j] <- sqrt(rho) * hub_signals[, hub_idx] + sqrt(1 - rho) * rnorm(n_samples, sd = noise_sd)
    }
    colnames(X) <- feat_names
    rownames(X) <- paste0("sample_", seq_len(n_samples))
  }

  # --- True coefficients ---
  true_coefs <- numeric(n_features)
  names(true_coefs) <- feat_names
  # Assign effects: some positive, some negative
  signs <- sample(c(-1, 1), n_informative, replace = TRUE)
  true_coefs[seq_len(n_informative)] <- signs * effect_size
  names(true_coefs) <- feat_names

  # --- Generate outcome ---
  linear_pred <- X[, true_names, drop = FALSE] %*%
    matrix(true_coefs[true_names], ncol = 1)
  noise <- rnorm(n_samples, sd = 0.5)
  lp <- as.numeric(linear_pred) + noise

  if (outcome_type == "binary") {
    prob <- 1 / (1 + exp(-lp))
    y <- rbinom(n_samples, 1, prob)
  } else {
    y <- lp
  }

  # --- Inject missing values ---
  if (missing_frac > 0) {
    n_missing <- round(n_samples * n_features * missing_frac)
    miss_idx <- sample(seq_len(n_samples * n_features), n_missing)
    X[miss_idx] <- NA_real_
  }

  list(X = X, y = y,
       true_features = true_names,
       true_coefficients = true_coefs[true_coefs != 0],
       metadata = list(
         n_samples = n_samples, n_features = n_features,
         n_informative = n_informative, outcome_type = outcome_type,
         effect_size = effect_size, cor_structure = cor_structure,
         seed = seed
       ))
}

#' Generate a named benchmark dataset
#'
#' Convenience wrapper that creates multiple benchmark scenarios.
#'
#' @param scenario Character. One of "easy", "medium", "hard", "high_dim".
#' @param seed Integer. Random seed.
#' @return List from create_synthetic_data.
#' @export
generate_benchmark <- function(scenario = c("easy", "medium", "hard", "high_dim"),
                                seed = 42) {
  scenario <- match.arg(scenario)
  params <- switch(scenario,
    easy    = list(n_samples = 200, n_features = 50,  n_informative = 5,
                   effect_size = 2.0, cor_structure = "independent"),
    medium  = list(n_samples = 200, n_features = 200, n_informative = 10,
                   effect_size = 1.5, cor_structure = "independent"),
    hard    = list(n_samples = 150, n_features = 500, n_informative = 15,
                   effect_size = 1.0, cor_structure = "block"),
    high_dim = list(n_samples = 100, n_features = 1000, n_informative = 10,
                    effect_size = 1.5, cor_structure = "hub")
  )
  do.call(create_synthetic_data, c(params, list(seed = seed)))
}

#' Get ground truth for a synthetic dataset
#'
#' @param data List from create_synthetic_data or generate_benchmark.
#' @return List with true_features and true_coefficients.
#' @export
get_benchmark_truth <- function(data) {
  list(true_features = data$true_features,
       true_coefficients = data$true_coefficients)
}
