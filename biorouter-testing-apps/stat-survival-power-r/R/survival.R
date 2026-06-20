#' Power and Sample Size for Survival / Log-Rank Tests
#'
#' Implements the Schoenfeld (1983) and Freedman (1982) formulas for
#' event counts and total sample size in two-arm time-to-event trials
#' with hazard ratio, allocation ratio, accrual, follow-up, and
#' dropout.
#'
#' @name survival_power
NULL

# ---------------------------------------------------------------------------
# Internal
# ---------------------------------------------------------------------------

# Probability of observing an event for a subject with
# exponential(hazard) entering at time t_acc and followed until T_fu
.event_prob_exp <- function(lambda, t_acc, T_fu) {
  # For exponential: P(event) = 1 - exp(-lambda * T_fu) on average
  # if all subjects are followed for full T_fu.
  # More precisely, for uniform accrual over [0, t_acc]:
  # each subject is followed for T_fu - t_i where t_i ~ U(0, t_acc)
  # Average follow-up: T_fu - t_acc/2 (if all survive)
  # P(event | entry at t) = 1 - exp(-lambda * (T_fu - t))
  # Average over t ~ U(0, t_acc):
  # (1/t_acc) * int_0^t_acc [1 - exp(-lambda*(T_fu - t))] dt
  # = 1 - (1/(lambda * t_acc)) * (exp(-lambda*(T_fu-t_acc)) - exp(-lambda*T_fu))
  if (lambda < 1e-15) return(0)
  1 - (exp(-lambda * (T_fu - t_acc)) - exp(-lambda * T_fu)) / (lambda * t_acc)
}

# ---------------------------------------------------------------------------
# Exported functions
# ---------------------------------------------------------------------------

#' Schoenfeld formula for number of events
#'
#' The number of events required for a log-rank test with given
#' hazard ratio and power:
#'
#'   d = (z_{alpha/2} + z_{beta})^2 / (log(HR))^2 * (1/p1 + 1/p2)
#'
#' where p1 and p2 are the allocation proportions.
#'
#' @param hr Hazard ratio (treatment / control).
#' @param power Desired power (1 - beta).
#' @param alpha Two-sided significance level.
#' @param p1 Proportion allocated to arm 1 (default 0.5).
#' @param p2 Proportion allocated to arm 2 (default 1 - p1).
#' @return Named list: \code{n_events} (ceiling), \code{z_alpha}, \code{z_beta}.
#' @export
#' @examples
#' power_survival_logrank(hr = 0.7, power = 0.80, alpha = 0.05)
power_survival_logrank <- function(hr, power = 0.80, alpha = 0.05,
                                    p1 = 0.5, p2 = 1 - p1,
                                    n_events = NULL,
                                    n = NULL,
                                    t_accrual = NULL,
                                    t_followup = NULL,
                                    dropout_rate = 0) {
  z_alpha <- qnorm(1 - alpha / 2)
  z_beta  <- qnorm(power)

  # --- Schoenfeld: solve for events given HR, power ---
  log_hr <- log(hr)
  events_schoenfeld <- (z_alpha + z_beta)^2 / log_hr^2 * (1 / p1 + 1 / p2)

  # --- Freedman: inflate for exponential survival with accrual/followup ---
  events_freedman <- events_schoenfeld
  if (!is.null(t_accrual) && !is.null(t_followup) && t_accrual > 0) {
    # Overall probability of event under exponential model
    # Average hazard = average of lambda1 and lambda2 weighted by allocation
    # We use the null overall hazard for sample-size inflation
    lambda_avg <- -log(0.5)  # assume median survival ~1 unit if not given
    p_event <- .event_prob_exp(lambda_avg, t_accrual, t_followup)
    if (p_event > 0) {
      events_freedman <- events_schoenfeld / p_event
    }
  }

  # Inflation for dropout (exponential censoring model)
  if (dropout_rate > 0) {
    events_freedman <- events_freedman / (1 - dropout_rate)
  }

  result <- list(
    n_events_schoenfeld = ceiling(events_schoenfeld),
    n_events_freedman   = ceiling(events_freedman),
    z_alpha = z_alpha,
    z_beta  = z_beta
  )

  # --- Solve for total N if allocation and follow-up are given ---
  if (!is.null(p1) && !is.null(t_accrual) && !is.null(t_followup)) {
    # N = n_events / (p_event * allocation fractions)
    # For equal allocation: each arm needs events / (2 * p_event_per_arm)
    lambda_for_n <- -log(0.5)
    p_event_arm <- .event_prob_exp(lambda_for_n, t_accrual, t_followup)
    if (p_event_arm > 0) {
      n_per_arm <- ceiling(events_freedman / (2 * p_event_arm))
      result$n_per_arm <- n_per_arm
      result$n_total <- 2 * n_per_arm
    }
  }

  result
}

#' Compute sample size for a log-rank test (convenience wrapper)
#'
#' @param hr Hazard ratio.
#' @param power Desired power.
#' @param alpha Significance level.
#' @param p1 Allocation proportion for arm 1.
#' @param p2 Allocation proportion for arm 2.
#' @param t_accrual Accrual period (time units).
#' @param t_followup Additional follow-up after last enrollment.
#' @param dropout_rate Proportion expected to be lost to follow-up.
#' @return Named list with \code{n_events}, \code{n_per_arm}, \code{n_total}.
#' @export
#' @examples
#' sample_size_survival_logrank(hr = 0.7, power = 0.80, t_accrual = 2, t_followup = 1)
sample_size_survival_logrank <- function(hr, power = 0.80, alpha = 0.05,
                                          p1 = 0.5, p2 = 1 - p1,
                                          t_accrual = 2, t_followup = 1,
                                          dropout_rate = 0) {
  power_survival_logrank(
    hr = hr, power = power, alpha = alpha,
    p1 = p1, p2 = p2,
    t_accrual = t_accrual, t_followup = t_followup,
    dropout_rate = dropout_rate
  )
}
