"""
Hierarchical Normal model example.

Model:
    y_{ij} ~ N(theta_j, sigma^2)   (observations in group j)
    theta_j ~ N(mu, tau^2)         (group means)
    mu ~ N(0, 100)                 (population mean)
    tau ~ HalfCauchy(5)            (population std, via Gamma approx)
    sigma ~ HalfCauchy(5)          (observation std, via Gamma approx)

This is a classic hierarchical model demonstrating partial pooling.
We simulate data from 5 groups and estimate group means.
"""

import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

import math
import numpy as np
from bayesmcmc.model import Model
from bayesmcmc.distributions import Normal, Gamma
from bayesmcmc.samplers import MetropolisHastings
from bayesmcmc.diagnostics import compute_rhat, compute_ess
from bayesmcmc.summary import posterior_summary, format_summary_table


def generate_hierarchical_data(
    n_groups=5,
    n_per_group=20,
    true_mu=5.0,
    true_tau=2.0,
    true_sigma=1.0,
    seed=42,
):
    """Generate hierarchical normal data."""
    rng = np.random.default_rng(seed)
    true_thetas = rng.normal(true_mu, true_tau, size=n_groups)
    y = np.zeros((n_groups, n_per_group))
    for j in range(n_groups):
        y[j] = rng.normal(true_thetas[j], true_sigma, size=n_per_group)
    return y, true_thetas


def hierarchical_log_posterior(data, mu, tau, sigma, *thetas, **kwargs):
    """Log posterior for hierarchical normal model."""
    # Extract group thetas from kwargs
    thetas = np.array([kwargs[f"theta_{j}"] for j in range(len(kwargs) if "theta_" in str(k) for k in kwargs)])
    # Reconstruct from flat dict
    theta_vals = []
    j = 0
    while f"theta_{j}" in kwargs:
        theta_vals.append(kwargs[f"theta_{j}"])
        j += 1
    thetas = np.array(theta_vals)

    if sigma <= 0 or tau <= 0:
        return -math.inf

    n_groups = data.shape[0]
    n_per = data.shape[1]

    # Priors
    lp = 0.0
    # mu ~ N(0, 10^2)
    lp += -0.5 * (mu / 10) ** 2 - math.log(10 * math.sqrt(2 * math.pi))
    # tau ~ Gamma(2, 0.5) (half-Cauchy-like)
    lp += (2 - 1) * math.log(tau) - 0.5 * tau - math.lgamma(2)
    # sigma ~ Gamma(2, 0.5)
    lp += (2 - 1) * math.log(sigma) - 0.5 * sigma - math.lgamma(2)

    # Group means
    lp += np.sum(-0.5 * ((thetas - mu) / tau) ** 2 - math.log(tau) - 0.5 * math.log(2 * math.pi))

    # Observations
    for j in range(n_groups):
        lp += np.sum(-0.5 * ((data[j] - thetas[j]) / sigma) ** 2 - math.log(sigma) - 0.5 * math.log(2 * math.pi))

    return lp


def main():
    # --- Generate data ---
    true_mu = 5.0
    true_tau = 2.0
    true_sigma = 1.0
    n_groups = 5
    n_per_group = 20

    y, true_thetas = generate_hierarchical_data(
        n_groups=n_groups, n_per_group=n_per_group,
        true_mu=true_mu, true_tau=true_tau, true_sigma=true_sigma,
    )

    print("=" * 70)
    print("HIERARCHICAL NORMAL MODEL")
    print("=" * 70)
    print(f"\nTrue parameters:")
    print(f"  mu={true_mu}, tau={true_tau}, sigma={true_sigma}")
    print(f"  Group means: {true_thetas}")
    print(f"  Data: {n_groups} groups, {n_per_group} observations each")

    # --- Build model with custom likelihood ---
    model = Model(name="hierarchical_normal")
    model.add_parameter("mu", Normal(0, 10))
    model.add_parameter("tau", Gamma(2, 0.5))
    model.add_parameter("sigma", Gamma(2, 0.5))
    for j in range(n_groups):
        model.add_parameter(f"theta_{j}", Normal(0, 10))

    model.set_likelihood(lambda data, **params: hierarchical_log_posterior(
        data,
        params["mu"],
        params["tau"],
        params["sigma"],
        **{k: v for k, v in params.items() if k.startswith("theta_")},
    ))
    model.set_data(y)

    n_samples = 3000
    n_chains = 3
    seed = 42

    # --- MH sampling ---
    print("\n" + "-" * 70)
    print("Metropolis-Hastings Sampler")
    step_sizes = {"mu": 0.3, "tau": 0.2, "sigma": 0.2}
    for j in range(n_groups):
        step_sizes[f"theta_{j}"] = 0.3

    mh_sampler = MetropolisHastings(model, step_sizes=step_sizes)
    mh_chains = mh_sampler.run(
        n_samples=n_samples, n_chains=n_chains, burn_in=1000, seed=seed
    )

    # --- Posterior summaries ---
    print("\nPosterior Summaries:")
    param_names = ["mu", "tau", "sigma"] + [f"theta_{j}" for j in range(n_groups)]
    summaries = {}
    for name in param_names:
        summaries[name] = posterior_summary(mh_chains[name].flatten())
    print(format_summary_table(summaries))

    # --- Diagnostics ---
    print("\nDiagnostics:")
    for name in param_names:
        rhat = compute_rhat(mh_chains, name)
        ess = compute_ess(mh_chains[name].flatten())
        print(f"  {name:<10}: R-hat={rhat:.4f}, ESS={ess:.0f}")
    print(f"  Acceptance rate: {mh_chains['_acceptance_rate'].mean():.3f}")

    # --- Comparison ---
    print("\n" + "=" * 70)
    print("COMPARISON WITH TRUE VALUES")
    print("=" * 70)
    print(f"  {'Parameter':<10} {'True':>10} {'Post. Mean':>12} {'95% CI':>22}")
    print("  " + "-" * 56)
    for name in param_names:
        if name.startswith("theta_"):
            j = int(name.split("_")[1])
            true_val = true_thetas[j]
        elif name == "mu":
            true_val = true_mu
        elif name == "tau":
            true_val = true_tau
        elif name == "sigma":
            true_val = true_sigma
        else:
            continue

        post = summaries[name]
        ci = f"[{post['ci_lower']:.3f}, {post['ci_upper']:.3f}]"
        print(f"  {name:<10} {true_val:>10.3f} {post['mean']:>12.4f} {ci:>22}")


if __name__ == "__main__":
    main()
