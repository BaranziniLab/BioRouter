#' Proportional Hazards Assumption Checking
#' 
#' Functions for testing the PH assumption using Schoenfeld residuals.
#' @name ph_assumption
NULL

#' Check Proportional Hazards Assumption
#' 
#' Tests the PH assumption using Schoenfeld residuals. A significant
#' correlation between scaled Schoenfeld residuals and transformed time
#' suggests violation of the PH assumption.
#' 
#' @param time Numeric vector of event/censor times
#' @param event Numeric binary vector (1=event, 0=censored)
#' @param X Numeric matrix or data.frame of covariates
#' @param beta Optional pre-computed coefficients (if NULL, estimates via Cox PH)
#' 
#' @return A list with components:
#' \describe{
#'   \item{test_statistic}{Vector of chi-square test statistics for each covariate}
#'   \item{p_value}{Vector of p-values for each covariate}
#'   \item{schoenfeld_residuals}{List of Schoenfeld residual matrices}
#'   \item{rho}{Correlation between residuals and transformed time}
#'   \item{conclusion}{Character vector indicating which covariates violate PH}
#' }
#' 
#' @export
check_ph_assumption <- function(time, event, X, beta = NULL) {
  # Validate inputs
  n <- length(time)
  if (length(event) != n) {
    stop("time and event must have the same length")
  }
  
  # Ensure X is matrix
  if (is.data.frame(X)) {
    X <- as.matrix(X)
  }
  if (is.null(dim(X))) {
    X <- matrix(X, ncol = 1)
  }
  p <- ncol(X)
  
  # Fit Cox PH model if beta not provided
  if (is.null(beta)) {
    fit <- cox_ph_model(time, event, X)
    beta <- fit$coefficients
  }
  
  # Compute Schoenfeld residuals
  schoenfeld_resid <- compute_schoenfeld_residuals(time, event, X, beta)
  
  # For each covariate, test correlation with transformed time
  test_stat <- numeric(p)
  p_val <- numeric(p)
  rho <- numeric(p)
  
  # Log time transform (log(t) - mean(log(t)))
  event_times <- time[event == 1]
  log_t <- log(event_times)
  log_t_centered <- log_t - mean(log_t)
  
  for (j in seq_len(p)) {
    resid_j <- schoenfeld_resid[, j]
    
    # Correlation test
    if (length(resid_j) > 2) {
      test <- tryCatch(
        stats::cor.test(log_t_centered, resid_j),
        error = function(e) {
          list(estimate = NA, p.value = NA, statistic = NA)
        }
      )
      rho[j] <- test$estimate
      test_stat[j] <- test$statistic^2  # Chi-square approximation
      p_val[j] <- test$p.value
    } else {
      rho[j] <- NA
      test_stat[j] <- NA
      p_val[j] <- NA
    }
  }
  
  # Overall test (joint)
  if (p > 1) {
    # Variance of rho
    rho_var <- var(rho, na.rm = TRUE)
    overall_stat <- sum(test_stat, na.rm = TRUE)
    overall_df <- sum(!is.na(test_stat))
    overall_p <- 1 - stats::pchisq(overall_stat, df = overall_df)
  } else {
    overall_stat <- test_stat
    overall_df <- 1
    overall_p <- p_val
  }
  
  # Conclusion
  alpha <- 0.05
  conclusion <- rep("PH assumption holds", p)
  conclusion[p_val < alpha] <- "PH assumption violated"
  
  list(
    test_statistic = setNames(test_stat, colnames(X)),
    p_value = setNames(p_val, colnames(X)),
    schoenfeld_residuals = schoenfeld_resid,
    rho = setNames(rho, colnames(X)),
    overall_test = list(
      statistic = overall_stat,
      df = overall_df,
      p_value = overall_p
    ),
    conclusion = setNames(conclusion, colnames(X))
  )
}

#' Compute Schoenfeld Residuals
#' 
#' Computes scaled Schoenfeld residuals for Cox PH model diagnostics.
#' 
#' @param time Numeric vector of event/censor times
#' @param event Numeric binary vector (1=event, 0=censored)
#' @param X Numeric matrix of covariates
#' @param beta Coefficient vector
#' 
#' @return Matrix of Schoenfeld residuals (n_events x p)
#' @keywords internal
compute_schoenfeld_residuals <- function(time, event, X, beta) {
  n <- length(time)
  p <- ncol(X)
  
  # Sort by time (descending)
  ord <- order(time, decreasing = TRUE)
  time <- time[ord]
  event <- event[ord]
  X <- X[ord, , drop = FALSE]
  
  # Compute risk scores
  XB <- X %*% beta
  exp_XB <- as.numeric(exp(XB))
  
  # Schoenfeld residuals at each event time
  event_idx <- which(event == 1)
  n_events <- length(event_idx)
  schoenfeld <- matrix(0, n_events, p)
  
  for (k in seq_len(n_events)) {
    i <- event_idx[k]
    risk_set <- which(time >= time[i])
    n_risk <- length(risk_set)
    
    # Weighted average of covariates in risk set
    weights <- exp_XB[risk_set] / sum(exp_XB[risk_set])
    weighted_X <- colSums(X[risk_set, , drop = FALSE] * weights)
    
    # Schoenfeld residual: observed - expected
    schoenfeld[k, ] <- X[i, ] - weighted_X
  }
  
  schoenfeld
}

#' PH assumption check using survival package (wrapper)
#' 
#' Alternative implementation using survival::cox.zph.
#' 
#' @param cox_model Output from survival::coxph
#' 
#' @return Output from survival::cox.zph
#' 
#' @export
check_ph_assumption_survival <- function(cox_model) {
  if (!requireNamespace("survival", quietly = TRUE)) {
    stop("survival package required")
  }
  
  survival::cox.zph(cox_model)
}
