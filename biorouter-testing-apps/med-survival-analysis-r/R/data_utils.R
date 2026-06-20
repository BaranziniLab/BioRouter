#' Data Loading and Preparation Utilities
#' 
#' Functions for loading, validating, and preparing survival analysis data.
#' @name data_utils
NULL

#' Load survival data from a CSV file or data frame
#' 
#' Reads and validates survival data with required columns: time, event,
#' and optional covariates.
#' 
#' @param source Character path to CSV file, or a data.frame
#' @param time_col Character name of the time column (default: "time")
#' @param event_col Character name of the event indicator column (default: "event")
#' @param group_col Character name of the grouping variable (optional)
#' @param covariate_cols Character vector of additional covariate column names
#' 
#' @return A list with components:
#' \describe{
#'   \item{data}{Cleaned data.frame with all variables}
#'   \item{time}{Numeric vector of event/censor times}
#'   \item{event}{Numeric binary vector (1=event, 0=censored)}
#'   \item{group}{Factor vector of group assignments (if provided)}
#'   \item{covariates}{Data.frame of covariates (if provided)}
#'   \item{n_subjects}{Number of subjects}
#'   \item{n_events}{Number of observed events}
#' }
#' 
#' @export
load_survival_data <- function(source, time_col = "time", event_col = "event",
                               group_col = NULL, covariate_cols = NULL) {
  # Load data
  if (is.character(source)) {
    if (!file.exists(source)) {
      stop("File not found: ", source)
    }
    data <- utils::read.csv(source, stringsAsFactors = FALSE)
  } else if (is.data.frame(source)) {
    data <- source
  } else {
    stop("source must be a file path or data.frame")
  }
  
  # Validate required columns
  required_cols <- c(time_col, event_col)
  missing_cols <- setdiff(required_cols, names(data))
  if (length(missing_cols) > 0) {
    stop("Missing required columns: ", paste(missing_cols, collapse = ", "))
  }
  
  # Extract and validate time
  time <- as.numeric(data[[time_col]])
  if (any(is.na(time)) || any(time < 0)) {
    stop("Time column must contain non-negative numeric values")
  }
  
  # Extract and validate event indicator
  event <- as.numeric(data[[event_col]])
  if (!all(event %in% c(0, 1))) {
    stop("Event column must contain only 0 (censored) or 1 (event)")
  }
  
  # Extract group if provided
  group <- NULL
  if (!is.null(group_col) && group_col %in% names(data)) {
    group <- as.factor(data[[group_col]])
  }
  
  # Extract covariates if provided
  covariates <- NULL
  if (!is.null(covariate_cols)) {
    valid_covs <- intersect(covariate_cols, names(data))
    if (length(valid_covs) > 0) {
      covariates <- data[, valid_covs, drop = FALSE]
      # Convert character columns to factors
      for (col in names(covariates)) {
        if (is.character(covariates[[col]])) {
          covariates[[col]] <- as.factor(covariates[[col]])
        }
      }
    }
  }
  
  list(
    data = data,
    time = time,
    event = event,
    group = group,
    covariates = covariates,
    n_subjects = length(time),
    n_events = sum(event)
  )
}

#' Summarize survival data
#' 
#' Provides descriptive statistics for survival data including event rates,
#' censoring summary, and time-to-event distribution.
#' 
#' @param surv_data Output from load_survival_data
#' @param group Logical whether to stratify by group (if available)
#' 
#' @return A list with summary statistics
#' 
#' @export
summarize_survival_data <- function(surv_data, group = TRUE) {
  time <- surv_data$time
  event <- surv_data$event
  group <- surv_data$group
  
  summary_list <- list(
    n_subjects = length(time),
    n_events = sum(event == 1),
    n_censored = sum(event == 0),
    event_rate = mean(event == 1),
    time_summary = summary(time),
    median_time = stats::median(time),
    min_time = min(time),
    max_time = max(time)
  )
  
  # Stratified by group if available
  if (!is.null(group)) {
    group_summary <- list()
    for (g in levels(group)) {
      idx <- which(group == g)
      group_summary[[g]] <- list(
        n = length(idx),
        n_events = sum(event[idx] == 1),
        event_rate = mean(event[idx] == 1),
        median_time = stats::median(time[idx])
      )
    }
    summary_list$by_group <- group_summary
  }
  
  summary_list
}

#' Generate synthetic survival data with known hazard ratio
#' 
#' Creates simulated survival data from exponential distributions with
#' known hazard ratio between groups, useful for testing.
#' 
#' @param n_per_group Number of subjects per group (default: 100)
#' @param base_hazard Baseline hazard rate for control group (default: 0.1)
#' @param hazard_ratio True hazard ratio (treatment vs control, default: 0.7)
#' @param censor_time Maximum follow-up time for right censoring (default: 5)
#' @param seed Random seed for reproducibility
#' 
#' @return A data.frame with columns: id, time, event, group, covariate1, covariate2
#' 
#' @export
generate_synthetic_survival <- function(n_per_group = 100, base_hazard = 0.1,
                                        hazard_ratio = 0.7, censor_time = 5,
                                        seed = 42) {
  set.seed(seed)
  
  n <- 2 * n_per_group
  
  # Generate group assignment
  group <- rep(c("control", "treatment"), each = n_per_group)
  
  # Calculate group-specific hazards
  lambda_control <- base_hazard
  lambda_treatment <- base_hazard * hazard_ratio
  
  lambda <- ifelse(group == "control", lambda_control, lambda_treatment)
  
  # Generate exponential survival times
  # Using inverse CDF: T = -log(U)/lambda where U ~ Uniform(0,1)
  u <- stats::runif(n)
  true_time <- -log(u) / lambda
  
  # Apply censoring (administrative censoring at censor_time)
  time <- pmin(true_time, censor_time)
  event <- as.numeric(true_time <= censor_time)
  
  # Generate some covariates
  covariate1 <- stats::rnorm(n, mean = 0, sd = 1)
  covariate2 <- stats::rbinom(n, size = 1, prob = 0.3)
  
  data.frame(
    id = 1:n,
    time = time,
    event = event,
    group = group,
    covariate1 = covariate1,
    covariate2 = covariate2,
    true_time = true_time,
    stringsAsFactors = FALSE
  )
}
