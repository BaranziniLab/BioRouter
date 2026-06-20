#' Cox Proportional Hazards Regression
#' 
#' Implements Cox PH model with Newton-Raphson optimization on the
#' partial likelihood for coefficient estimation.
#' @name cox_ph
NULL

#' Cox Proportional Hazards Model
#' 
#' Fits a Cox proportional hazards model using Newton-Raphson optimization
#' with step-halving on the partial likelihood. Returns hazard ratios,
#' confidence intervals, and Wald test statistics.
#' 
#' @param time Numeric vector of event/censor times
#' @param event Numeric binary vector (1=event, 0=censored)
#' @param X Numeric matrix or data.frame of covariates
#' @param conf_level Confidence level for intervals (default: 0.95)
#' @param max_iter Maximum iterations for Newton-Raphson (default: 200)
#' @param tol Convergence tolerance (default: 1e-6)
#' 
#' @return A list with components:
#' \describe{
#'   \item{coefficients}{Estimated regression coefficients (beta)}
#'   \item{hazard_ratios}{exp(beta) - hazard ratios}
#'   \item{se}{Standard errors of coefficients}
#'   \item{z}{Wald z-statistics}
#'   \item{p_value}{P-values from Wald test}
#'   \item{ci_lower}{Lower confidence bound for HR}
#'   \item{ci_upper}{Upper confidence bound for HR}
#'   \item{log_likelihood}{Maximized partial log-likelihood}
#'   \item{converged}{Logical indicating convergence}
#'   \item{n_iterations}{Number of iterations used}
#'   \item{concordance}{Concordance index (C-statistic)}
#' }
#' 
#' @export
cox_ph_model <- function(time, event, X, conf_level = 0.95,
                         max_iter = 200, tol = 1e-6) {
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
  if (ncol(X) == 0) {
    stop("X must have at least one covariate column")
  }
  if (nrow(X) != n) {
    stop("X must have same number of rows as time/event")
  }

  # Remove any NAs
  complete <- complete.cases(X)
  if (!all(complete)) {
    warning("Removing rows with missing covariates")
    time <- time[complete]
    event <- event[complete]
    X <- X[complete, , drop = FALSE]
    n <- length(time)
    if (n == 0) stop("No complete cases after removing NAs")
  }

  p <- ncol(X)
  z_val <- stats::qnorm(1 - (1 - conf_level) / 2)

  # Sort data by time (ascending) for risk set computation
  ord <- order(time, decreasing = FALSE)
  time <- time[ord]
  event <- event[ord]
  X <- X[ord, , drop = FALSE]

  # Compute risk set indices (precomputed for efficiency)
  risk_sets <- vector("list", n)
  for (i in seq_len(n)) {
    risk_sets[[i]] <- which(time >= time[i])
  }

  # Evaluate partial log-likelihood, score, and Hessian at given beta
  eval_pl <- function(beta) {
    XB <- as.numeric(X %*% beta)
    exp_XB <- exp(XB)

    log_lik <- 0
    score <- numeric(p)
    hessian <- matrix(0, p, p)

    for (i in seq_len(n)) {
      if (event[i] == 1) {
        rs <- risk_sets[[i]]
        sum_exp <- sum(exp_XB[rs])

        if (sum_exp > 0) {
          log_lik <- log_lik + XB[i] - log(sum_exp)

          wX <- colSums(X[rs, , drop = FALSE] * exp_XB[rs]) / sum_exp
          score <- score + X[i, ] - wX

          wX2 <- crossprod(X[rs, , drop = FALSE] * exp_XB[rs]) / sum_exp
          hessian <- hessian - (wX2 - outer(wX, wX))
        }
      }
    }

    list(log_lik = log_lik, score = score, hessian = hessian, exp_XB = exp_XB, XB = XB)
  }

  # Initialize coefficients at zero
  beta <- rep(0, p)

  # Newton-Raphson with step-halving
  converged <- FALSE
  log_lik_old <- -Inf
  iter <- 0

  for (iter in seq_len(max_iter)) {
    ev <- eval_pl(beta)

    # Check convergence
    if (abs(ev$log_lik - log_lik_old) < tol * (1 + abs(ev$log_lik))) {
      converged <- TRUE
      break
    }
    log_lik_old <- ev$log_lik

    # Try Newton step with step-halving
    tryCatch({
      delta <- solve(ev$hessian, ev$score)
    }, error = function(e) {
      delta <<- numeric(p)  # Stay at current beta if singular
    })

    step_size <- 1.0
    beta_new <- beta - step_size * delta
    ev_new <- eval_pl(beta_new)

    # Step halving: reduce step until likelihood improves
    for (halving in 1:10) {
      if (ev_new$log_lik > ev$log_lik - 1e-10) break
      step_size <- step_size / 2
      beta_new <- beta - step_size * delta
      ev_new <- eval_pl(beta_new)
    }

    beta <- beta_new
  }

  if (!converged) {
    warning("Newton-Raphson did not converge after ", max_iter, " iterations")
  }

  # Final evaluation
  ev <- eval_pl(beta)

  # Standard errors from inverse Hessian
  se <- rep(NA, p)
  tryCatch({
    vcov <- solve(-ev$hessian)
    se <- sqrt(abs(diag(vcov)))
  }, error = function(e) {
    warning("Could not compute standard errors: ", e$message)
  })

  # Wald statistics
  z_stat <- beta / se
  p_value <- 2 * stats::pnorm(-abs(z_stat))

  # Hazard ratios and confidence intervals
  hr <- exp(beta)
  ci_lower <- exp(beta - z_val * se)
  ci_upper <- exp(beta + z_val * se)

  # Concordance index
  concordance <- compute_concordance(time, event, ev$XB)

  list(
    coefficients = setNames(beta, colnames(X)),
    hazard_ratios = setNames(hr, colnames(X)),
    se = setNames(se, colnames(X)),
    z = setNames(z_stat, colnames(X)),
    p_value = setNames(p_value, colnames(X)),
    ci_lower = setNames(ci_lower, colnames(X)),
    ci_upper = setNames(ci_upper, colnames(X)),
    log_likelihood = ev$log_lik,
    converged = converged,
    n_iterations = iter,
    concordance = concordance
  )
}

#' Compute concordance index (efficient implementation)
#' 
#' Calculates Harrell's C-statistic for model discrimination.
#' 
#' @param time Numeric vector of event times
#' @param event Numeric binary vector (1=event, 0=censored)
#' @param score Numeric vector of risk scores (linear predictor)
#' 
#' @return Concordance index (C-statistic)
#' @keywords internal
compute_concordance <- function(time, event, score) {
  # Remove NAs
  ok <- !is.na(score)
  time <- time[ok]
  event <- event[ok]
  score <- score[ok]

  n <- length(time)
  if (n < 2) return(NA_real_)

  concordant <- 0
  tied <- 0
  total <- 0

  for (i in seq_len(n - 1)) {
    for (j in (i + 1):n) {
      # Skip pairs where we cannot determine ordering
      if (time[i] == time[j] && event[i] == 1 && event[j] == 1) next
      if (event[i] == 0 && event[j] == 0) next

      if (time[i] < time[j] && event[i] == 1) {
        # i has worse outcome (died earlier)
        total <- total + 1
        if (score[i] > score[j]) concordant <- concordant + 1
        else if (score[i] == score[j]) tied <- tied + 1
      } else if (time[j] < time[i] && event[j] == 1) {
        # j has worse outcome
        total <- total + 1
        if (score[j] > score[i]) concordant <- concordant + 1
        else if (score[i] == score[j]) tied <- tied + 1
      } else if (time[i] == time[j]) {
        # Same time, one event one censored: event counts as worse
        if (event[i] == 1 && event[j] == 0) {
          total <- total + 1
          if (score[i] > score[j]) concordant <- concordant + 1
          else if (score[i] == score[j]) tied <- tied + 1
        } else if (event[j] == 1 && event[i] == 0) {
          total <- total + 1
          if (score[j] > score[i]) concordant <- concordant + 1
          else if (score[i] == score[j]) tied <- tied + 1
        }
      }
    }
  }

  if (total == 0) return(NA_real_)
  (concordant + 0.5 * tied) / total
}

#' Cox PH Model using survival package (wrapper)
#' 
#' Alternative implementation using survival::coxph.
#' 
#' @param formula Formula (e.g., Surv(time, event) ~ x1 + x2)
#' @param data Data frame containing the variables
#' 
#' @return Output from survival::coxph
#' 
#' @export
cox_ph_model_survival <- function(formula, data) {
  if (!requireNamespace("survival", quietly = TRUE)) {
    stop("survival package required")
  }
  survival::coxph(formula, data = data)
}
