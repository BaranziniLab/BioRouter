# ---------------------------------------------------------------------------
# sim-data.R — synthetic data with known coefficients for validation
# True betos are stored as attributes: attr(dat, "true_beta")
# ---------------------------------------------------------------------------

simulate_gaussian_data = function(n = 200, seed = 42) {
  set.seed(seed)
  x1 = rnorm(n)
  x2 = rnorm(n)
  beta0 = 2.0
  beta1 = -1.5
  beta2 = 0.8
  mu = beta0 + beta1 * x1 + beta2 * x2
  y = mu + rnorm(n, sd = 0.5)
  dat = data.frame(y = y, x1 = x1, x2 = x2)
  attr(dat, "true_beta") = c(beta0, beta1, beta2)
  dat
}

simulate_binomial_data = function(n = 200, seed = 123) {
  set.seed(seed)
  x1 = rnorm(n)
  x2 = rnorm(n)
  beta0 = -0.5
  beta1 = 1.2
  beta2 = -0.7
  eta = beta0 + beta1 * x1 + beta2 * x2
  prob = 1 / (1 + exp(-eta))
  y = rbinom(n, 1, prob)
  dat = data.frame(y = y, x1 = x1, x2 = x2)
  attr(dat, "true_beta") = c(beta0, beta1, beta2)
  dat
}

simulate_poisson_data = function(n = 200, seed = 456) {
  set.seed(seed)
  x1 = rnorm(n)
  x2 = rnorm(n)
  beta0 = 0.5
  beta1 = 0.3
  beta2 = -0.2
  eta = beta0 + beta1 * x1 + beta2 * x2
  mu = exp(eta)
  y = rpois(n, mu)
  dat = data.frame(y = y, x1 = x1, x2 = x2)
  attr(dat, "true_beta") = c(beta0, beta1, beta2)
  dat
}

simulate_factor_data = function(n = 200, seed = 789) {
  set.seed(seed)
  group = factor(sample(c("A", "B", "C"), n, replace = TRUE))
  x1 = rnorm(n)
  beta0 = 1.0
  betaB = 0.5
  betaC = -0.3
  beta1 = 0.8
  mu = beta0 + betaB * (group == "B") + betaC * (group == "C") + beta1 * x1
  y = mu + rnorm(n, sd = 0.5)
  dat = data.frame(y = y, group = group, x1 = x1)
  attr(dat, "true_beta") = c(beta0, betaB, betaC, beta1)
  dat
}
