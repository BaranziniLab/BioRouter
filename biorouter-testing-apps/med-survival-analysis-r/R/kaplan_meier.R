#' Kaplan-Meier Survival Estimation
#' 
#' Functions for non-parametric survival estimation using the Kaplan-Meier method.
#' @name kaplan_meier
NULL

#' Kaplan-Meier survival estimate
#' 
#' Computes the Kaplan-Meier estimator of the survival function with
#' Greenwood's variance and confidence intervals.
#' 
#' @param time Numeric vector of event/censor times
#' @param event Numeric binary vector (1=event, 0=censored)
#' @param group Optional factor for stratified analysis
#' @param conf_level Confidence level for intervals (default: 0.95)
#' 
#' @return A list with components:
#' \describe{
#'   \item{times}{Sorted unique event times}
#'   \item{survival}{Survival probability estimates at each time}
#'   \item{variance}{Greenwood variance estimates}
#'   \item{se}{Standard error of survival estimates}
#'   \item{lower}{Lower confidence bound}
#'   \item{upper}{Upper confidence bound}
#'   \item{n_at_risk}{Number at risk before each event time}
#'   \item{n_events}{Number of events at each time}
#'   \item{n_censored}{Number censored at each time}
#'   \item{median_survival}{Median survival time (NA if not reached)}
#'   \item{median_ci}{95% CI for median survival}
#'   \item{n_subjects}{Total number of subjects}
#'   \item{n_total_events}{Total number of events}
#' }
#' 
#' @export
km_estimate <- function(time, event, group = NULL, conf_level = 0.95) {
  # Validate inputs
  if (length(time) != length(event)) {
    stop("time and event must have the same length")
  }
  
  n <- length(time)
  z <- stats::qnorm(1 - (1 - conf_level) / 2)
  
  # Handle grouped analysis
  if (!is.null(group)) {
    if (length(group) != n) {
      stop("group must have the same length as time and event")
    }
    groups <- levels(as.factor(group))
    result <- list()
    for (g in groups) {
      idx <- which(group == g)
      result[[as.character(g)]] <- km_estimate_single(time[idx], event[idx], z)
    }
    result$groups <- groups
    result$grouped <- TRUE
    return(result)
  }
  
  # Single group analysis
  km_estimate_single(time, event, z)
}

#' Internal: Single group KM estimation
#' @keywords internal
km_estimate_single <- function(time, event, z) {
  n <- length(time)
  
  # Sort by time
  ord <- order(time)
  time <- time[ord]
  event <- event[ord]
  
  # Get unique event times (only times where events occurred)
  event_times <- sort(unique(time[event == 1]))
  
  # Initialize output vectors
  k <- length(event_times)
  times <- numeric(k)
  survival <- numeric(k)
  variance <- numeric(k)
  se <- numeric(k)
  lower <- numeric(k)
  upper <- numeric(k)
  n_at_risk <- numeric(k)
  n_events <- numeric(k)
  n_censored <- numeric(k)
  
  # Kaplan-Meier computation
  S <- 1  # Start with survival = 1
  V <- 0  # Greenwood variance accumulator
  
  for (j in seq_along(event_times)) {
    t_j <- event_times[j]
    
    # Number at risk just before time t_j
    d_j <- sum(time == t_j & event == 1)  # deaths at t_j
    c_j <- sum(time == t_j & event == 0)  # censored at t_j
    n_j <- sum(time >= t_j)  # at risk
    
    # Update KM estimate
    if (n_j > 0) {
      S <- S * (1 - d_j / n_j)
      # Greenwood variance
      V <- V + (d_j / (n_j * (n_j - d_j))) * (1 - S)^2 / S^2
    }
    
    times[j] <- t_j
    survival[j] <- S
    variance[j] <- V
    se[j] <- sqrt(V)
    lower[j] <- max(0, S - z * se[j])
    upper[j] <- min(1, S + z * se[j])
    n_at_risk[j] <- n_j
    n_events[j] <- d_j
    n_censored[j] <- c_j
  }
  
  # Calculate median survival (first time where S <= 0.5)
  median_survival <- NA
  median_ci <- c(NA, NA)
  
  idx_median <- which(survival <= 0.5)
  if (length(idx_median) > 0) {
    median_survival <- times[min(idx_median)]
    
    # CI for median: use inverted test or simple interpolation
    idx_lower <- which(lower <= 0.5)
    idx_upper <- which(upper <= 0.5)
    
    if (length(idx_lower) > 0) {
      median_ci[2] <- times[min(idx_lower)]
    } else {
      median_ci[2] <- Inf
    }
    
    if (length(idx_upper) > 0) {
      median_ci[1] <- times[max(idx_upper)]
    } else {
      median_ci[1] <- times[min(idx_median)]
    }
  }
  
  list(
    times = times,
    survival = survival,
    variance = variance,
    se = se,
    lower = lower,
    upper = upper,
    n_at_risk = n_at_risk,
    n_events = n_events,
    n_censored = n_censored,
    median_survival = median_survival,
    median_ci = median_ci,
    n_subjects = n,
    n_total_events = sum(event == 1),
    grouped = FALSE
  )
}

#' Prepare Kaplan-Meier data for plotting
#' 
#' Creates a data.frame suitable for plotting KM curves with ggplot2.
#' 
#' @param km_result Output from km_estimate
#' @param group Optional group label for multi-group plots
#' 
#' @return A data.frame with columns: time, survival, lower, upper, group
#' 
#' @export
km_plot_data <- function(km_result, group = NULL) {
  if (km_result$grouped) {
    # Combine all groups into single data.frame
    plot_data <- data.frame()
    for (g in km_result$groups) {
      km_g <- km_result[[g]]
      df <- data.frame(
        time = km_g$times,
        survival = km_g$survival,
        lower = km_g$lower,
        upper = km_g$upper,
        group = g,
        stringsAsFactors = FALSE
      )
      
      # Add time 0 with survival = 1
      df <- rbind(
        data.frame(time = 0, survival = 1, lower = 1, upper = 1, group = g),
        df
      )
      
      plot_data <- rbind(plot_data, df)
    }
    plot_data$group <- factor(plot_data$group, levels = km_result$groups)
    return(plot_data)
  }
  
  # Single group
  df <- data.frame(
    time = km_result$times,
    survival = km_result$survival,
    lower = km_result$lower,
    upper = km_result$upper,
    group = if (!is.null(group)) group else "Overall",
    stringsAsFactors = FALSE
  )
  
  # Add time 0
  rbind(
    data.frame(time = 0, survival = 1, lower = 1, upper = 1, group = df$group[1]),
    df
  )
}
