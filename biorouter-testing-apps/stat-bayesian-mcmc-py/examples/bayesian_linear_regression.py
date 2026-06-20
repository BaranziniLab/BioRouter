"""
Bayesian Linear Regression example.

Model:
    y_i = beta_0 + beta_1 * x_i + eps_i
    eps_i ~ N(0, sigma^2)

Priors:
    beta_j ~ N(0, 10^2)
    sigma  ~ Gamma(2, 2)

We demonstrate:
1. Model construction using Model.linear_regression()
2. MH sampling with adaptive proposals
3. Posterior summaries and diagnostics
4. Comparison with OLS estimates
"""

import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

import numpy as np
from bayesmcmc.model import Model
from bayesmcmc.samplers import MetropolisHastings, HMCSampler
from bayesmcmc.diagnostics import compute_rhat, compute_ess
from bayesmcmc.summary import (
    posterior_summary,
    format_summary_table,
    format_trace_ascii,
    format_histogram_ascii,
)


def generate_data(n=50, beta_0=2.0, beta_1=3.0, sigma=1.0, seed=42):
    """Generate synthetic linear regression data."""
    rng = np.random.default_rng(seed)
    x = rng.uniform(-2, 2, size=n)
    y = beta_0 + beta_1 * x + rng.normal(0, sigma, size=n)
    return x, y


def main():
    # --- Generate data ---
    true_beta_0, true_beta_1, true_sigma = 2.0, 3.0, 1.0
    x, y = generate_data(n=50, beta_0=true_beta_0, beta_1=true_beta_1, sigma=true_sigma)

    # --- OLS for comparison ---
    X = np.column_stack([np.ones(len(x)), x])
    ols_betas = np.linalg.lstsq(X, y, rcond=None)[0]
    ols_sigma = np.sqrt(np.sum((y - X @ ols_betas) ** 2) / (len(y) - 2))

    print("=" * 70)
    print("BAYESIAN LINEAR REGRESSION")
    print("=" * 70)
    print(f"\nTrue parameters: beta_0={true_beta_0}, beta_1={true_beta_1}, sigma={true_sigma}")
    print(f"OLS estimates:   beta_0={ols_betas[0]:.4f}, beta_1={ols_betas[1]:.4f}, sigma={ols_sigma:.4f}")
    print(f"Data: n={len(y)}, y range [{y.min():.2f}, {y.max():.2f}]")

    # --- Build model ---
    model = Model.linear_regression(X, y, sigma_prior=10.0, noise_prior_alpha=2.0, noise_prior_beta=2.0)

    n_samples = 5000
    n_chains = 4
    seed = 42

    # --- MH with adaptive proposals ---
    print("\n" + "-" * 70)
    print("Metropolis-Hastings (Adaptive)")
    mh_sampler = MetropolisHastings(model, step_sizes={"beta_0": 0.5, "beta_1": 0.5, "sigma": 0.3})
    mh_chains = mh_sampler.run(
        n_samples=n_samples, n_chains=n_chains, burn_in=2000, thin=2, seed=seed
    )

    summaries = {}
    for name in ["beta_0", "beta_1", "sigma"]:
        summaries[name] = posterior_summary(mh_chains[name].flatten())
    print(format_summary_table(summaries))
    for name in ["beta_0", "beta_1", "sigma"]:
        print(f"  {name} R-hat: {compute_rhat(mh_chains, name):.4f}, "
              f"ESS: {compute_ess(mh_chains[name].flatten()):.0f}")
    print(f"  Acceptance rate: {mh_chains['_acceptance_rate'].mean():.3f}")

    # --- Trace plots ---
    print("\n" + "-" * 70)
    print("Trace Plots (chain 0)")
    for name in ["beta_0", "beta_1", "sigma"]:
        print(format_trace_ascii(mh_chains[name][0], title=f"{name} (chain 0)"))
        print()

    # --- Histograms ---
    print("-" * 70)
    print("Posterior Histograms (pooled)")
    for name in ["beta_0", "beta_1", "sigma"]:
        print(format_histogram_ascii(mh_chains[name].flatten(), title=name))
        print()

    # --- Comparison ---
    print("=" * 70)
    print("COMPARISON WITH OLS")
    print("=" * 70)
    print(f"  {'Parameter':<10} {'True':>10} {'OLS':>10} {'Post. Mean':>12} {'95% CI':>22}")
    print("  " + "-" * 66)
    for i, name in enumerate(["beta_0", "beta_1", "sigma"]):
        true = [true_beta_0, true_beta_1, true_sigma][i]
        ols = [ols_betas[0], ols_betas[1], ols_sigma][i]
        post = summaries[name]
        ci = f"[{post['ci_lower']:.3f}, {post['ci_upper']:.3f}]"
        print(f"  {name:<10} {true:>10.4f} {ols:>10.4f} {post['mean']:>12.4f} {ci:>22}")


if __name__ == "__main__":
    main()
