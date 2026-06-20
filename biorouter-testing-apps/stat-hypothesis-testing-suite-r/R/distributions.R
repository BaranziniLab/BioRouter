#' Statistical distribution functions implemented from scratch.
#' Provides t, F, chi-squared, normal, and Wilcoxon distributions.
#' These serve as the CDF/p-value machinery for our tests.

# ---- Normal Distribution ----

#' Standard normal CDF using error function approximation
#' @param q Numeric: quantile
#' @return Probability P(Z <= q)
#' @export
norm_cdf <- function(q) {
  0.5 * (1 + erf(q / sqrt(2)))
}

#' Normal distribution PDF
#' @param x Numeric: value
#' @return Density at x
#' @export
norm_pdf <- function(x) {
  exp(-0.5 * x^2) / sqrt(2 * pi)
}

#' Error function (Abramowitz and Stegun approximation)
#' @param x Numeric
#' @return erf(x)
erf <- function(x) {
  # High-precision polynomial approximation (Abramowitz & Stegun 7.1.26)
  sign_x <- sign(x)
  x <- abs(x)
  t <- 1 / (1 + 0.3275911 * x)
  t2 <- t * t
  t3 <- t2 * t
  t4 <- t3 * t
  t5 <- t4 * t
  poly <- 0.254829592 * t - 0.284496736 * t2 + 1.421413741 * t3 -
    1.453152027 * t4 + 1.061405429 * t5
  result <- 1 - poly * exp(-x * x)
  return(sign_x * result)
}

# ---- t Distribution ----

#' t-distribution density
#' @param t_val Numeric: t-statistic value
#' @param df Numeric: degrees of freedom
#' @return Density at t_val
#' @export
t_pdf <- function(t_val, df) {
  exp(lgamma((df + 1) / 2) - lgamma(df / 2) - 0.5 * log(df * pi) -
    ((df + 1) / 2) * log(1 + t_val^2 / df))
}

#' t-distribution CDF using regularized incomplete beta function
#' @param q Numeric: quantile
#' @param df Numeric: degrees of freedom
#' @return Probability P(T <= q)
#' @export
t_cdf <- function(q, df) {
  if (abs(q) < 1e-10) return(0.5)
  x <- df / (df + q^2)
  # I_x(a,b) = regularized incomplete beta function
  beta_val <- regbeta(df / 2, 0.5, x)
  if (q >= 0) {
    return(1 - 0.5 * beta_val)
  } else {
    return(0.5 * beta_val)
  }
}

# ---- F Distribution ----

#' F-distribution density
#' @param f_val Numeric: F-statistic value
#' @param df1 Numeric: numerator df
#' @param df2 Numeric: denominator df
#' @return Density at f_val
#' @export
f_pdf <- function(f_val, df1, df2) {
  if (f_val <= 0) return(0)
  lnum <- (df1 / 2) * log(df1) + (df2 / 2) * log(df2) +
    ((df1 - 1) / 2) * log(f_val) -
    lgamma(df1 / 2) - lgamma(df2 / 2) +
    lgamma((df1 + df2) / 2)
  ldenom <- ((df1 + df2) / 2) * log(df2 + df1 * f_val)
  exp(lnum - ldenom)
}

#' F-distribution CDF using regularized incomplete beta function
#' @param q Numeric: quantile
#' @param df1 Numeric: numerator df
#' @param df2 Numeric: denominator df
#' @return Probability P(F <= q)
#' @export
f_cdf <- function(q, df1, df2) {
  if (q <= 0) return(0)
  x <- df1 * q / (df1 * q + df2)
  return(regbeta(df1 / 2, df2 / 2, x))
}

# ---- Chi-Squared Distribution ----

#' Chi-squared CDF using regularized incomplete gamma function
#' @param q Numeric: quantile
#' @param df Numeric: degrees of freedom
#' @return Probability P(X^2 <= q)
#' @export
chisq_cdf <- function(q, df) {
  if (q <= 0) return(0)
  return(reggamma(df / 2, q / 2))
}

# ---- Regularized Incomplete Beta Function ----

#' Regularized incomplete beta function I_x(a, b)
#' Uses continued fraction via Lentz's method
#' @param a Numeric: shape parameter 1 (must be > 0)
#' @param b Numeric: shape parameter 2 (must be > 0)
#' @param x Numeric: value in [0, 1]
#' @return I_x(a, b)
#' @export
regbeta <- function(a, b, x) {
  if (x < 0 || x > 1) stop("x must be in [0, 1]")
  if (x == 0) return(0)
  if (x == 1) return(1)

  # Use continued fraction for I_x(a,b)
  # Based on Numerical Recipes implementation
  lbeta_val <- lgamma(a) + lgamma(b) - lgamma(a + b)

  if (x < (a + 1) / (a + b + 2)) {
    # Use continued fraction directly
    front <- exp(a * log(x) + b * log(1 - x) - lbeta_val) / a
    return(front * cf_beta(a, b, x))
  } else {
    # Use 1 - I_{1-x}(b,a) for better numerical stability
    front <- exp(b * log(1 - x) + a * log(x) - lbeta_val) / b
    return(1 - front * cf_beta(b, a, 1 - x))
  }
}

#' Continued fraction for regularized incomplete beta
#' @param a shape parameter
#' @param b shape parameter
#' @param x value in [0, 1]
#' @return I_x(a,b) without the front factor
cf_beta <- function(a, b, x) {
  max_iter <- 200
  eps <- 1e-14
  qab <- a + b
  qap <- a + 1
  qam <- a - 1

  # First step
  c <- 1
  d <- 1 - qab * x / qap
  if (abs(d) < 1e-30) d <- 1e-30
  d <- 1 / d
  h <- d

  for (m in 1:max_iter) {
    m2 <- 2 * m

    # Even step
    aa <- m * (b - m) * x / ((qam + m2) * (a + m2))
    d <- 1 + aa * d
    if (abs(d) < 1e-30) d <- 1e-30
    c <- 1 + aa / c
    if (abs(c) < 1e-30) c <- 1e-30
    d <- 1 / d
    h <- h * d * c

    # Odd step
    aa <- -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2))
    d <- 1 + aa * d
    if (abs(d) < 1e-30) d <- 1e-30
    c <- 1 + aa / c
    if (abs(c) < 1e-30) c <- 1e-30
    d <- 1 / d
    del <- d * c
    h <- h * del

    if (abs(del - 1) < eps) break
  }

  return(h)
}

# ---- Regularized Lower Incomplete Gamma Function ----

#' Regularized lower incomplete gamma function P(a, x)
#' Uses series expansion
#' @param a Numeric: shape parameter (> 0)
#' @param x Numeric: value (>= 0)
#' @return P(a, x)
#' @export
reggamma <- function(a, x) {
  if (x < 0) stop("x must be >= 0")
  if (x == 0) return(0)

  if (x < a + 1) {
    # Series expansion
    ap <- a
    sum_val <- 1 / a
    delta <- 1 / a
    for (n in 1:300) {
      ap <- ap + 1
      delta <- delta * x / ap
      sum_val <- sum_val + delta
      if (abs(delta) < abs(sum_val) * 1e-15) break
    }
    return(sum_val * exp(-x + a * log(x) - lgamma(a)))
  } else {
    # Continued fraction (Lentz's method)
    f <- 1 - a
    b <- x + 1 - a
    c <- 1e30
    d <- 1 / b
    h <- d

    for (i in 1:300) {
      an <- -i * (i - a)
      b <- b + 2
      d <- an * d + b
      if (abs(d) < 1e-30) d <- 1e-30
      d <- 1 / d
      c <- b + an / c
      if (abs(c) < 1e-30) c <- 1e-30
      delta <- d * c
      h <- h * delta
      if (abs(delta - 1) < 1e-15) break
    }
    return(1 - h * exp(-x + a * log(x) - lgamma(a)))
  }
}

# ---- Wilcoxon Signed-Rank Distribution ----

#' CDF of Wilcoxon signed-rank statistic (exact for small n)
#' @param w Numeric: test statistic
#' @param n Integer: sample size (excluding zeros)
#' @param alternative Character: "two.sided", "less", or "greater"
#' @return P-value
#' @export
wilcox_cdf <- function(w, n, alternative = "two.sided") {
  # Use normal approximation for n > 20
  if (n > 20) {
    mu <- n * (n + 1) / 4
    sigma <- sqrt(n * (n + 1) * (2 * n + 1) / 24)
    p_val <- 1 - norm_cdf((w - mu) / sigma)
    if (alternative == "two.sided") p_val <- 2 * min(p_val, 1 - p_val)
    return(p_val)
  }

  # Exact enumeration for small n
  max_w <- n * (n + 1) / 2
  probs <- numeric(max_w + 1)
  probs[1] <- 1  # W = 0

  # Dynamic programming
  for (k in 1:n) {
    new_probs <- probs
    for (w_val in 0:max_w) {
      if (probs[w_val + 1] > 0) {
        new_w <- w_val + k
        if (new_w <= max_w) {
          new_probs[new_w + 1] <- new_probs[new_w + 1] + probs[w_val + 1]
        }
      }
    }
    probs <- new_probs
  }

  total <- sum(probs)
  probs <- probs / total

  if (alternative == "less") {
    return(sum(probs[1:(floor(w) + 1)]))
  } else if (alternative == "greater") {
    return(sum(probs[(floor(w) + 1):(max_w + 1)]))
  } else {
    # two-sided: 2 * min(P(W <= w), P(W >= w))
    p_lower <- sum(probs[1:(floor(w) + 1)])
    p_upper <- sum(probs[(ceiling(w)):(max_w + 1)])
    return(2 * min(p_lower, p_upper))
  }
}

# ---- Rank-sum Distribution ----

#' P-value for Wilcoxon rank-sum test using normal approximation
#' @param w Numeric: test statistic (sum of ranks)
#' @param n1 Integer: size of group 1
#' @param n2 Integer: size of group 2
#' @param alternative Character
#' @return P-value
#' @export
ranksum_normal_approx <- function(w, n1, n2, alternative = "two.sided") {
  mu <- n1 * (n1 + n2 + 1) / 2
  sigma <- sqrt(n1 * n2 * (n1 + n2 + 1) / 12)
  z <- (w - mu) / sigma
  p_val <- 1 - norm_cdf(z)
  if (alternative == "two.sided") p_val <- 2 * min(p_val, 1 - p_val)
  return(p_val)
}
