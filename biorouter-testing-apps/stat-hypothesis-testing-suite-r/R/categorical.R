#' Categorical Data Tests
#'
#' Implements chi-square, Fisher's exact, and McNemar tests from scratch.

# ---- Chi-Square Goodness-of-Fit ----

#' Chi-square goodness-of-fit test
#'
#' @param observed Numeric vector of observed frequencies
#' @param expected Numeric vector of expected frequencies (default: uniform)
#' @return A tidy_result object
#' @export
hyp_chi_square_gof <- function(observed, expected = NULL) {
  observed <- as.numeric(observed)
  k <- length(observed)

  if (is.null(expected)) {
    expected <- rep(sum(observed) / k, k)
  }

  if (length(expected) != k) stop("observed and expected must have same length")
  if (any(expected <= 0)) stop("Expected frequencies must be positive")

  # Chi-square statistic
  chisq_stat <- sum((observed - expected)^2 / expected)
  df <- k - 1
  p_val <- 1 - chisq_cdf(chisq_stat, df)

  # Effect size: Cramer's V (for GoF, V = sqrt(chisq / n))
  n <- sum(observed)
  cramers_v <- sqrt(chisq_stat / n)

  return(tidy_result(
    test_name = "Chi-Square Goodness-of-Fit",
    statistic = chisq_stat,
    df = df,
    p_value = p_val,
    effect_size = cramers_v,
    effect_name = "Cramer's V",
    method = "Chi-square goodness-of-fit (from scratch)",
    extra = list(n = n, k = k, observed = observed, expected = expected)
  ))
}

# ---- Chi-Square Test of Independence ----

#' Chi-square test of independence
#'
#' @param x A matrix or data frame (contingency table)
#' @return A tidy_result object
#' @export
hyp_chi_square_independence <- function(x) {
  # Ensure we have a matrix
  if (is.data.frame(x)) x <- as.matrix(x)

  row_sums <- rowSums(x)
  col_sums <- colSums(x)
  n <- sum(x)
  r <- nrow(x)
  c <- ncol(x)

  # Expected frequencies
  expected <- outer(row_sums, col_sums) / n

  # Chi-square statistic
  chisq_stat <- sum((x - expected)^2 / expected)
  df <- (r - 1) * (c - 1)
  p_val <- 1 - chisq_cdf(chisq_stat, df)

  # Effect sizes
  # Cramer's V
  min_dim <- min(r, c)
  cramers_v <- sqrt(chisq_stat / (n * (min_dim - 1)))

  # Phi coefficient (for 2x2)
  phi <- NA
  if (r == 2 && c == 2) {
    phi <- sqrt(chisq_stat / n)
  }

  return(tidy_result(
    test_name = "Chi-Square Test of Independence",
    statistic = chisq_stat,
    df = df,
    p_value = p_val,
    effect_size = cramers_v,
    effect_name = "Cramer's V",
    method = "Chi-square test of independence (from scratch)",
    extra = list(n = n, rows = r, cols = c, phi = phi, expected = expected)
  ))
}

# ---- Fisher's Exact Test ----

#' Fisher's exact test for 2x2 contingency tables
#'
#' Uses hypergeometric distribution for exact calculation.
#'
#' @param x A 2x2 matrix or data frame
#' @param alternative Character: "two.sided", "less", or "greater"
#' @return A tidy_result object
#' @export
hyp_fisher_exact <- function(x, alternative = "two.sided") {
  if (is.data.frame(x)) x <- as.matrix(x)
  if (nrow(x) != 2 || ncol(x) != 2) stop("Fisher's exact test requires a 2x2 table")

  # Get cell values
  a <- x[1, 1]
  b <- x[1, 2]
  c <- x[2, 1]
  d <- x[2, 2]
  n <- a + b + c + d

  # Hypergeometric distribution parameters
  m <- a + b   # row 1 total
  k <- a + c   # col 1 total
  N <- n

  # The probability of observing this table or more extreme
  if (alternative == "less") {
    # P(X <= a) for hypergeometric
    p_val <- phyper_hyp(a, m, N - m, k)
  } else if (alternative == "greater") {
    # P(X >= a)
    p_val <- 1 - phyper_hyp(a - 1, m, N - m, k)
  } else {
    # Two-sided: sum probabilities <= P(observed)
    p_obs <- dhyper_hyp(a, m, N - m, k)
    p_val <- 0
    for (x_val in max(0, k - (N - m)):min(k, m)) {
      p_x <- dhyper_hyp(x_val, m, N - m, k)
      if (p_x <= p_obs + 1e-15) {
        p_val <- p_val + p_x
      }
    }
  }

  # Odds ratio
  odds_ratio <- (a * d) / (b * c)

  # Confidence interval for odds ratio (Woolf log method)
  if (a > 0 && b > 0 && c > 0 && d > 0) {
    log_or <- log(odds_ratio)
    se_log_or <- sqrt(1/a + 1/b + 1/c + 1/d)
    ci_lower <- exp(log_or - 1.96 * se_log_or)
    ci_upper <- exp(log_or + 1.96 * se_log_or)
  } else {
    ci_lower <- NA
    ci_upper <- NA
  }

  return(tidy_result(
    test_name = "Fisher's Exact Test",
    statistic = odds_ratio,
    df = 1,
    p_value = p_val,
    effect_size = odds_ratio,
    effect_name = "Odds Ratio",
    ci_lower = ci_lower,
    ci_upper = ci_upper,
    alternative = alternative,
    method = "Fisher's exact test for 2x2 tables (from scratch)",
    extra = list(
      cells = c(a = a, b = b, c = c, d = d),
      n = n
    )
  ))
}

# ---- McNemar's Test ----

#' McNemar's test for paired nominal data
#'
#' @param x A 2x2 contingency table (before/after)
#' @return A tidy_result object
#' @export
hyp_mcnemar <- function(x) {
  if (is.data.frame(x)) x <- as.matrix(x)
  if (nrow(x) != 2 || ncol(x) != 2) stop("McNemar's test requires a 2x2 table")

  # McNemar statistic
  # chi^2 = (b - c)^2 / (b + c)
  # where table is [[a, b], [c, d]]
  b <- x[1, 2]
  c <- x[2, 1]

  # With continuity correction
  chi_sq <- (abs(b - c) - 1)^2 / (b + c)

  # Without continuity correction (standard McNemar)
  chi_sq_nocorr <- (b - c)^2 / (b + c)

  df <- 1
  p_val <- 1 - chisq_cdf(chi_sq_nocorr, df)

  # Exact binomial p-value for small samples
  n_discordant <- b + c
  if (n_discordant <= 25) {
    # Use exact binomial test
    p_exact <- 2 * binom_test_pvalue(min(b, c), n_discordant, "two.sided")
    p_val <- p_exact
  }

  # Effect size: odds ratio = b / c (for discordant pairs)
  if (c > 0) {
    odds_ratio <- b / c
  } else {
    odds_ratio <- Inf
  }

  return(tidy_result(
    test_name = "McNemar's Test",
    statistic = chi_sq_nocorr,
    df = df,
    p_value = p_val,
    effect_size = odds_ratio,
    effect_name = "Odds Ratio (discordant)",
    method = "McNemar's test for paired nominal data (from scratch)",
    extra = list(
      b = b, c = c, n_discordant = n_discordant,
      chi_sq_corrected = chi_sq
    )
  ))
}

# ---- Helper: Hypergeometric distribution ----

dhyper_hyp <- function(x, m, n, k) {
  # P(X = x) for hypergeometric(m, n, k)
  if (x < max(0, k - n) || x > min(m, k)) return(0)
  exp(lchoose(m, x) + lchoose(n, k - x) - lchoose(m + n, k))
}

phyper_hyp <- function(x, m, n, k) {
  # P(X <= x) for hypergeometric
  if (x < 0) return(0)
  x_min <- max(0, k - n)
  x_max <- min(x, min(m, k))
  if (x_max < x_min) return(0)
  p <- 0
  for (i in x_min:x_max) {
    p <- p + dhyper_hyp(i, m, n, k)
  }
  return(p)
}
