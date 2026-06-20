#' Non-Parametric Hypothesis Tests
#'
#' Implements rank-based and distribution-free tests from scratch.

# ---- Wilcoxon Rank-Sum Test ----

#' Wilcoxon rank-sum test (Mann-Whitney U test)
#'
#' @param x Numeric vector (group 1)
#' @param y Numeric vector (group 2)
#' @param alternative Character: "two.sided", "less", or "greater"
#' @return A tidy_result object
#' @export
hyp_wilcoxon_rank_sum <- function(x, y, alternative = "two.sided") {
  x <- x[!is.na(x)]
  y <- y[!is.na(y)]
  n1 <- length(x)
  n2 <- length(y)

  # Combine and rank
  all_vals <- c(x, y)
  groups <- c(rep(1, n1), rep(2, n2))
  ranks <- rank(all_vals)

  # Sum of ranks for group 1
  w <- sum(ranks[groups == 1])

  # Mann-Whitney U
  u1 <- w - n1 * (n1 + 1) / 2
  u2 <- n1 * n2 - u1
  u_stat <- min(u1, u2)

  # Normal approximation for p-value
  p_val <- ranksum_normal_approx(w, n1, n2, alternative)

  # Effect size: rank-biserial correlation
  r <- 1 - (2 * u_stat) / (n1 * n2)

  return(tidy_result(
    test_name = "Wilcoxon Rank-Sum Test",
    statistic = u_stat,
    df = c(n1, n2),
    p_value = p_val,
    effect_size = r,
    effect_name = "Rank-biserial r",
    alternative = alternative,
    method = "Wilcoxon rank-sum test / Mann-Whitney U (from scratch)",
    extra = list(W = w, U1 = u1, U2 = u2, n1 = n1, n2 = n2)
  ))
}

# ---- Wilcoxon Signed-Rank Test ----

#' Wilcoxon signed-rank test (paired, one-sample)
#'
#' @param x Numeric vector (pre/test scores, or differences)
#' @param y Numeric vector or NULL (post/control scores)
#' @param alternative Character: "two.sided", "less", or "greater"
#' @return A tidy_result object
#' @export
hyp_wilcoxon_signed_rank <- function(x, y = NULL, alternative = "two.sided") {
  if (!is.null(y)) {
    if (length(x) != length(y)) stop("x and y must have the same length")
    d <- x - y
  } else {
    d <- x
  }

  # Remove zeros and NAs
  d <- d[!is.na(d) & d != 0]
  n <- length(d)
  if (n < 1) stop("No non-zero differences")

  # Rank absolute values
  abs_d <- abs(d)
  ranks <- rank(abs_d)

  # Sum of positive ranks
  w_plus <- sum(ranks[d > 0])
  w_minus <- sum(ranks[d < 0])
  w_stat <- min(w_plus, w_minus)

  # For one-sample: W statistic for the test
  if (alternative == "less") {
    w_test <- w_minus
  } else if (alternative == "greater") {
    w_test <- w_plus
  } else {
    w_test <- w_plus  # standard W statistic
  }

  # P-value from exact distribution (small n) or normal approximation
  p_val <- wilcox_cdf(w_test, n, alternative)

  # Effect size: r = Z / sqrt(N)
  mu <- n * (n + 1) / 4
  sigma <- sqrt(n * (n + 1) * (2 * n + 1) / 24)
  z <- (w_plus - mu) / sigma
  effect <- z / sqrt(n)

  return(tidy_result(
    test_name = "Wilcoxon Signed-Rank Test",
    statistic = w_test,
    df = n,
    p_value = p_val,
    effect_size = effect,
    effect_name = "r (effect size)",
    alternative = alternative,
    method = "Wilcoxon signed-rank test (from scratch)",
    extra = list(W_plus = w_plus, W_minus = w_minus, z_approx = z, n = n)
  ))
}

# ---- Kruskal-Wallis Test ----

#' Kruskal-Wallis H test (non-parametric one-way ANOVA)
#'
#' @param formula Formula of the form y ~ group
#' @param data A data frame
#' @return A tidy_result object
#' @export
hyp_kruskal_wallis <- function(formula, data) {
  mf <- model.frame(formula, data = data)
  y <- model.response(mf)
  groups <- mf[, 2]
  group_levels <- unique(groups)
  k <- length(group_levels)
  n <- length(y)

  if (k < 2) stop("Need at least 2 groups")

  # Combined ranking
  all_ranks <- rank(y)

  # Compute H statistic
  group_sizes <- numeric(k)
  rank_sums <- numeric(k)

  for (i in seq_along(group_levels)) {
    idx <- groups == group_levels[i]
    group_sizes[i] <- sum(idx)
    rank_sums[i] <- sum(all_ranks[idx])
  }

  # H = [12 / (n(n+1))] * sum(R_i^2 / n_i) - 3(n+1)
  h_stat <- (12 / (n * (n + 1))) * sum(rank_sums^2 / group_sizes) - 3 * (n + 1)

  # df = k - 1
  df <- k - 1
  p_val <- 1 - chisq_cdf(h_stat, df)

  # Effect size: epsilon-squared
  eta2 <- h_stat / ((n^2 - 1) / (n))

  return(tidy_result(
    test_name = "Kruskal-Wallis Test",
    statistic = h_stat,
    df = df,
    p_value = p_val,
    effect_size = eta2,
    effect_name = "Epsilon-squared",
    method = "Kruskal-Wallis H test (from scratch)",
    extra = list(k = k, n = n, group_sizes = group_sizes,
                 rank_sums = rank_sums)
  ))
}

# ---- Mann-Whitney U Test (alias for rank-sum) ----

#' Mann-Whitney U test (explicit implementation)
#'
#' @param x Numeric vector (group 1)
#' @param y Numeric vector (group 2)
#' @param alternative Character: "two.sided", "less", or "greater"
#' @return A tidy_result object
#' @export
hyp_mann_whitney <- function(x, y, alternative = "two.sided") {
  # This is the same test as Wilcoxon rank-sum
  result <- hyp_wilcoxon_rank_sum(x, y, alternative)
  result$test_name <- "Mann-Whitney U Test"
  result$method <- "Mann-Whitney U test (from scratch)"
  return(result)
}

# ---- Spearman Rank Correlation ----

#' Spearman rank correlation coefficient test
#'
#' @param x Numeric vector
#' @param y Numeric vector
#' @param alternative Character: "two.sided", "less", or "greater"
#' @return A tidy_result object
#' @export
hyp_spearman_rho <- function(x, y, alternative = "two.sided") {
  complete <- complete.cases(x, y)
  x <- x[complete]
  y <- y[complete]
  n <- length(x)
  if (n < 3) stop("Need at least 3 paired observations")

  # Rank both variables
  rx <- rank(x)
  ry <- rank(y)

  # Pearson correlation on ranks
  m_rx <- mean(rx)
  m_ry <- mean(ry)
  num <- sum((rx - m_rx) * (ry - m_ry))
  den <- sqrt(sum((rx - m_rx)^2) * sum((ry - m_ry)^2))
  rho <- num / den

  # t-test for correlation
  t_stat <- rho * sqrt((n - 2) / (1 - rho^2))
  df <- n - 2

  if (alternative == "two.sided") {
    p_val <- 2 * (1 - t_cdf(abs(t_stat), df))
  } else if (alternative == "less") {
    p_val <- t_cdf(t_stat, df)
  } else {
    p_val <- 1 - t_cdf(t_stat, df)
  }

  return(tidy_result(
    test_name = "Spearman Rank Correlation",
    statistic = rho,
    df = df,
    p_value = p_val,
    effect_size = rho,
    effect_name = "rho",
    alternative = alternative,
    method = "Spearman rank correlation (from scratch)",
    extra = list(n = n, t_stat = t_stat)
  ))
}

# ---- Sign Test ----

#' Sign test (non-parametric paired comparison)
#'
#' @param x Numeric vector (pre/test scores)
#' @param y Numeric vector (post/control scores)
#' @param alternative Character: "two.sided", "less", or "greater"
#' @return A tidy_result object
#' @export
hyp_sign_test <- function(x, y, alternative = "two.sided") {
  if (length(x) != length(y)) stop("x and y must have the same length")

  diffs <- x - y
  # Remove zeros
  diffs <- diffs[!is.na(diffs) & diffs != 0]
  n <- length(diffs)
  if (n < 1) stop("No non-zero differences")

  n_pos <- sum(diffs > 0)
  n_neg <- sum(diffs < 0)

  # Binomial test: under H0, p = 0.5
  # Use exact binomial distribution
  p_val <- binom_test_pvalue(n_pos, n, alternative)

  # Effect size: proportion
  p_hat <- n_pos / n

  return(tidy_result(
    test_name = "Sign Test",
    statistic = n_pos,
    df = n,
    p_value = p_val,
    effect_size = p_hat,
    effect_name = "Proportion positive",
    alternative = alternative,
    method = "Sign test (from scratch)",
    extra = list(n_pos = n_pos, n_neg = n_neg, n = n)
  ))
}

# ---- Helper: Exact binomial test p-value ----

binom_test_pvalue <- function(k, n, alternative) {
  # P(X = k) under binomial(n, 0.5)
  dbinom_val <- exp(lchoose(n, k) + k * log(0.5) + (n - k) * log(0.5))

  if (alternative == "two.sided") {
    # Sum all probabilities <= P(X = k)
    probs <- sapply(0:n, function(i) exp(lchoose(n, i) + i * log(0.5) + (n - i) * log(0.5)))
    threshold <- dbinom_val
    p_val <- sum(probs[probs <= threshold + 1e-15])
  } else if (alternative == "less") {
    p_val <- sum(sapply(0:k, function(i) exp(lchoose(n, i) + i * log(0.5) + (n - i) * log(0.5))))
  } else {
    p_val <- sum(sapply(k:n, function(i) exp(lchoose(n, i) + i * log(0.5) + (n - i) * log(0.5))))
  }

  return(min(1, p_val))
}
