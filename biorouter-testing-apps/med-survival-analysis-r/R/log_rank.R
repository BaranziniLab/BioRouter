#' Log-Rank Test for Comparing Survival Curves
#' 
#' Implements the log-rank test (Mantel-Cox test) for comparing survival
#' between two or more groups.
#' @name log_rank
NULL

#' Log-Rank Test
#' 
#' Performs the log-rank test to compare survival distributions between groups.
#' Uses the Mantel-Haenszel chi-square statistic.
#' 
#' @param time Numeric vector of event/censor times
#' @param event Numeric binary vector (1=event, 0=censored)
#' @param group Factor vector of group assignments
#' @param alternative Alternative hypothesis: "two.sided", "greater", or "less"
#' 
#' @return A list with components:
#' \describe{
#'   \item{statistic}{Chi-square test statistic}
#'   \item{df}{Degrees of freedom}
#'   \item{p_value}{P-value}
#'   \item{n_per_group}{Number of subjects per group}
#'   \item{events_per_group}{Number of events per group}
#'   \item{expected_per_group}{Expected events under null hypothesis}
#' }
#' 
#' @export
log_rank_test <- function(time, event, group, alternative = "two.sided") {
  # Validate inputs
  if (length(time) != length(event) || length(time) != length(group)) {
    stop("time, event, and group must have the same length")
  }
  
  group <- as.factor(group)
  groups <- levels(group)
  g <- length(groups)
  
  if (g < 2) {
    stop("Need at least 2 groups for log-rank test")
  }
  
  n <- length(time)
  
  # Sort by time
  ord <- order(time)
  time <- time[ord]
  event <- event[ord]
  group <- group[ord]
  
  # Get unique event times
  event_times <- sort(unique(time[event == 1]))
  k <- length(event_times)
  
  # Initialize accumulators for each group
  O <- numeric(g)  # Observed events
  E <- numeric(g)  # Expected events
  V_matrix <- matrix(0, g, g)  # Variance-covariance
  
  # Score test statistic accumulators
  U <- numeric(g - 1)  # Score statistics
  
  for (j in seq_along(event_times)) {
    t_j <- event_times[j]
    
    # At risk just before t_j
    at_risk <- time >= t_j
    n_j <- sum(at_risk)
    
    # Events and censoring at t_j
    d_j <- sum(time == t_j & event == 1)
    c_j <- sum(time == t_j & event == 0)
    
    # Number at risk per group
    n_g <- numeric(g)
    d_g <- numeric(g)
    for (i in seq_len(g)) {
      n_g[i] <- sum(at_risk & group == groups[i])
      d_g[i] <- sum(time == t_j & event == 1 & group == groups[i])
    }
    
    # Observed events
    O <- O + d_g
    
    # Expected events under null (proportional)
    if (n_j > 1) {
      for (i in seq_len(g)) {
        E[i] <- E[i] + n_g[i] * d_j / n_j
      }
    }
    
    # Variance matrix (log-rank variance)
    if (n_j > 1) {
      p_g <- n_g / n_j
      d_total <- d_j
      
      # Variance of difference between groups 1 and 2
      # V = sum(d_j * (1 - p_j) * p_j * (n_j - d_j) / (n_j - 1))
      p1 <- p_g[1]
      p2 <- p_g[2]
      v <- d_total * (1 - p1) * p1 * (n_j - d_total) / (n_j - 1)
      V_matrix[1, 1] <- V_matrix[1, 1] + v
      V_matrix[2, 2] <- V_matrix[2, 2] + v
      V_matrix[1, 2] <- V_matrix[1, 2] - v
      V_matrix[2, 1] <- V_matrix[2, 1] - v
    }
  }
  
  # Compute test statistic
  # For two groups: chi-square = (O1 - E1)^2 / V
  if (g == 2) {
    chi_sq <- (O[1] - E[1])^2 / V_matrix[1, 1]
    df <- 1
    
    # Score test (log-rank statistic with sign for one-sided)
    z_score <- (O[1] - E[1]) / sqrt(V_matrix[1, 1])
  } else {
    # Multi-group: general chi-square
    diff <- O - E
    # Use generalized inverse if V is singular
    V_inv <- tryCatch(
      solve(V_matrix),
      error = function(e) MASS::ginv(V_matrix)
    )
    chi_sq <- as.numeric(t(diff) %*% V_inv %*% diff)
    df <- g - 1
    z_score <- NA
  }
  
  # P-values
  p_value <- 1 - stats::pchisq(chi_sq, df = df)
  
  # One-sided p-value
  if (alternative == "greater") {
    p_value <- 1 - stats::pnorm(z_score)
  } else if (alternative == "less") {
    p_value <- stats::pnorm(z_score)
  }
  
  # Group summaries
  n_per_group <- numeric(g)
  events_per_group <- numeric(g)
  for (i in seq_len(g)) {
    idx <- which(group == groups[i])
    n_per_group[i] <- length(idx)
    events_per_group[i] <- sum(event[idx] == 1)
  }
  
  names(O) <- groups
  names(E) <- groups
  
  list(
    statistic = chi_sq,
    df = df,
    p_value = p_value,
    z_score = z_score,
    alternative = alternative,
    n_per_group = setNames(n_per_group, groups),
    events_per_group = setNames(events_per_group, groups),
    expected_per_group = E,
    observed_per_group = O,
    variance = V_matrix[1:min(2, g), 1:min(2, g)]
  )
}

#' Log-rank test using survival package (wrapper)
#' 
#' Alternative implementation using the survival::survdiff function.
#' 
#' @param time Numeric vector of event/censor times
#' @param event Numeric binary vector (1=event, 0=censored)
#' @param group Factor vector of group assignments
#' 
#' @return Output from survival::survdiff
#' 
#' @export
log_rank_test_survival <- function(time, event, group) {
  if (!requireNamespace("survival", quietly = TRUE)) {
    stop("survival package required")
  }
  
  surv_obj <- survival::Surv(time, event)
  result <- survival::survdiff(surv_obj ~ group)
  result
}
