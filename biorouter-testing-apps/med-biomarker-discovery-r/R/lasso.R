#' LASSO and Elastic-Net feature selection
#'
#' Coordinate-descent implementation of LASSO / elastic-net logistic regression
#' for binary outcomes. No dependency on glmnet.

#' Soft-thresholding operator
#'
#' @param z Numeric scalar.
#' @param lambda Non-negative penalty.
#' @return Soft-thresholded value.
#' @keywords internal
soft_threshold <- function(z, lambda) {
  sign(z) * max(abs(z) - lambda, 0)
}

#' Coordinate-descent LASSO / elastic-net logistic regression
#'
#' Fits a logistic model with L1 (and optionally L2) penalty via
#' cyclic coordinate descent.
#'
#' @param X Numeric matrix (n x p), features scaled.
#' @param y Integer vector of 0/1 outcomes.
#' @param lambda Numeric. L1 penalty strength (default 0.1).
#' @param alpha Numeric in [0,1]. Elastic-net mixing: 1 = pure LASSO, 0 = ridge (default 1).
#' @param intercept Logical. Fit intercept? (default TRUE).
#' @param max_iter Integer. Maximum coordinate-descent iterations (default 1000).
#' @param tol Numeric. Convergence tolerance (default 1e-6).
#' @param standardize Logical. Internally standardize X? (default FALSE; assume already scaled).
#' @return List with components:
#'   \item{beta}{Numeric vector of length p: fitted coefficients.}
#'   \item{intercept}{Scalar intercept.}
#'   \item{lambda}{Used lambda.}
#'   \item{alpha}{Used alpha.}
#'   \item{iterations}{Number of iterations until convergence.}
#' @export
fit_lasso <- function(X, y, lambda = 0.1, alpha = 1,
                       intercept = TRUE, max_iter = 1000,
                       tol = 1e-6, standardize = FALSE) {
  if (!is.matrix(X)) X <- as.matrix(X)
  n <- nrow(X)
  p <- ncol(X)

  if (standardize) {
    mu <- col_means(X)
    sds <- apply(X, 2, sd)
    sds[sds == 0] <- 1
    X <- sweep(X, 2, mu)
    X <- sweep(X, 2, sds, "/")
  }

  beta <- numeric(p)
  names(beta) <- colnames(X)
  b0 <- 0

  for (iter in seq_len(max_iter)) {
    beta_old <- beta
    for (j in seq_len(p)) {
      # Working residual
      eta <- b0 + X[, j] * beta[j]
      if (intercept && iter == 1 && j == 1) {
        b0 <- sum(y - 0.5) / n  # initial intercept
        eta <- b0 + X[, j] * beta[j]
      }
      p_j <- 1 / (1 + exp(-clip(eta, -30, 30)))
      # Gradient without j-th term
      r_j <- (y - p_j) + X[, j] * beta[j]
      z_j <- sum(X[, j] * r_j) / n
      # Elastic-net penalty
      l1 <- lambda * alpha
      l2 <- lambda * (1 - alpha) * 2
      beta[j] <- soft_threshold(z_j, l1) / (sum(X[, j]^2) / n + l2)
    }
    # Update intercept
    if (intercept) {
      eta_full <- X %*% beta
      b0 <- sum(y - 1 / (1 + exp(-clip(eta_full, -30, 30)))) / n
    }
    # Convergence check
    if (max(abs(beta - beta_old)) < tol) break
  }

  list(beta = beta, intercept = b0, lambda = lambda, alpha = alpha,
       iterations = iter)
}

#' Predict from a fitted lasso model
#'
#' @param model List from fit_lasso.
#' @param X_new Numeric matrix.
#' @return Numeric vector of probabilities.
#' @export
predict_lasso <- function(model, X_new) {
  if (!is.matrix(X_new)) X_new <- as.matrix(X_new)
  eta <- model$intercept + X_new %*% model$beta
  1 / (1 + exp(-clip(as.numeric(eta), -30, 30)))
}

#' Select features with non-zero LASSO coefficients
#'
#' @param model List from fit_lasso.
#' @return Character vector of selected feature names.
#' @export
lasso_selected <- function(model) {
  if (is.null(names(model$beta))) {
    names(model$beta) <- paste0("feat_", seq_along(model$beta))
  }
  names(model$beta)[abs(model$beta) > 1e-10]
}

#' Fit ridge logistic regression (alpha = 0)
#'
#' @inheritParams fit_lasso
#' @return Same list structure as fit_lasso.
#' @export
fit_ridge <- function(X, y, lambda = 0.1, max_iter = 1000, tol = 1e-6) {
  fit_lasso(X, y, lambda = lambda, alpha = 0,
            max_iter = max_iter, tol = tol)
}
