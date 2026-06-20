"""
CLI driver for bayesmcmc.

Runs a built-in or custom model and prints posterior summaries,
diagnostics, and ASCII trace/histogram plots.

Usage:
    python -m bayesmcmc --model beta_binomial --data "1,1,1,0,0"
    python -m bayesmcmc --model linear_regression
    python -m bayesmcmc --model hierarchical_normal
"""

from __future__ import annotations

import argparse
import sys
import os
from typing import List, Optional

import numpy as np


def parse_args(argv: Optional[List[str]] = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="bayesmcmc",
        description="Bayesian inference and MCMC sampling",
    )
    parser.add_argument(
        "--model",
        choices=["beta_binomial", "linear_regression", "hierarchical_normal"],
        default="beta_binomial",
        help="Built-in model to run (default: beta_binomial)",
    )
    parser.add_argument(
        "--data",
        type=str,
        default=None,
        help="Comma-separated data values (for beta_binomial)",
    )
    parser.add_argument(
        "--n-samples",
        type=int,
        default=5000,
        help="Number of MCMC samples (default: 5000)",
    )
    parser.add_argument(
        "--n-chains",
        type=int,
        default=4,
        help="Number of MCMC chains (default: 4)",
    )
    parser.add_argument(
        "--burn-in",
        type=int,
        default=1000,
        help="Burn-in samples to discard (default: 1000)",
    )
    parser.add_argument(
        "--thin",
        type=int,
        default=2,
        help="Thinning factor (default: 2)",
    )
    parser.add_argument(
        "--seed",
        type=int,
        default=42,
        help="Random seed (default: 42)",
    )
    parser.add_argument(
        "--sampler",
        choices=["mh", "gibbs", "hmc", "slice"],
        default="mh",
        help="Sampler to use (default: mh)",
    )
    parser.add_argument(
        "--ci-level",
        type=float,
        default=0.95,
        help="Credible interval level (default: 0.95)",
    )
    parser.add_argument(
        "--quiet",
        action="store_true",
        help="Suppress ASCII plots",
    )
    return parser.parse_args(argv)


def run_beta_binomial(args):
    """Run the beta-binomial model."""
    from bayesmcmc.model import Model
    from bayesmcmc.distributions import Beta
    from bayesmcmc.samplers import MetropolisHastings, GibbsSampler, SliceSampler
    from bayesmcmc.diagnostics import compute_rhat, compute_ess
    from bayesmcmc.summary import posterior_summary, format_summary_table

    # parse data
    if args.data:
        data = np.array([float(x.strip()) for x in args.data.split(",")])
    else:
        # default: 7 successes in 10 trials
        data = np.array([1, 1, 1, 1, 1, 1, 1, 0, 0, 0])

    print("=" * 70)
    print("BETA-BINOMIAL MODEL")
    print("=" * 70)
    print(f"\nData: {int(data.sum())} successes in {len(data)} trials")

    # analytic posterior
    alpha_prior, beta_prior = 1.0, 1.0
    k = data.sum()
    n = len(data)
    alpha_post = alpha_prior + k
    beta_post = beta_prior + n - k
    print(f"Analytic Posterior: Beta({alpha_post}, {beta_post})")
    print(f"Analytic Mean: {alpha_post / (alpha_post + beta_post):.4f}")

    # build model
    model = Model.beta_binomial(alpha_prior, beta_prior)
    model.set_data(data)

    # select sampler
    if args.sampler == "gibbs":
        full_cond = GibbsSampler.beta_binomial_conditionals(alpha_prior, beta_prior)
        sampler = GibbsSampler(model, full_conditionals=full_cond)
        sampler_name = "Gibbs"
    elif args.sampler == "slice":
        sampler = SliceSampler(model, width=0.3)
        sampler_name = "Slice"
    else:
        sampler = MetropolisHastings(model, step_sizes={"p": 0.1})
        sampler_name = "Metropolis-Hastings"

    print(f"\nUsing {sampler_name} sampler")

    # run
    chains = sampler.run(
        n_samples=args.n_samples,
        n_chains=args.n_chains,
        burn_in=args.burn_in,
        thin=args.thin,
        seed=args.seed,
    )

    # summaries
    summary = posterior_summary(chains["p"].flatten(), ci_level=args.ci_level)
    print("\n" + format_summary_table({"p": summary}))
    print(f"\n  R-hat: {compute_rhat(chains, 'p'):.4f}")
    print(f"  ESS: {compute_ess(chains['p'].flatten()):.0f}")
    if "_acceptance_rate" in chains:
        print(f"  Acceptance rate: {chains['_acceptance_rate'].mean():.3f}")

    # ASCII plots
    if not args.quiet:
        from bayesmcmc.summary import format_trace_ascii, format_histogram_ascii
        print()
        print(format_trace_ascii(chains["p"][0], title="Trace (chain 0)"))
        print()
        print(format_histogram_ascii(chains["p"].flatten(), title="Posterior"))

    return chains


def run_linear_regression(args):
    """Run the Bayesian linear regression model."""
    from bayesmcmc.model import Model
    from bayesmcmc.samplers import MetropolisHastings
    from bayesmcmc.diagnostics import compute_rhat, compute_ess
    from bayesmcmc.summary import posterior_summary, format_summary_table

    # generate synthetic data
    rng = np.random.default_rng(args.seed)
    n = 50
    x = rng.uniform(-2, 2, size=n)
    true_b0, true_b1, true_sig = 2.0, 3.0, 1.0
    y = true_b0 + true_b1 * x + rng.normal(0, true_sig, size=n)

    X = np.column_stack([np.ones(n), x])

    print("=" * 70)
    print("BAYESIAN LINEAR REGRESSION")
    print("=" * 70)
    print(f"\nTrue: beta_0={true_b0}, beta_1={true_b1}, sigma={true_sig}")
    print(f"Data: n={n}")

    model = Model.linear_regression(X, y, sigma_prior=10.0, noise_prior_alpha=2.0, noise_prior_beta=2.0)

    step_sizes = {"beta_0": 0.5, "beta_1": 0.5, "sigma": 0.3}
    sampler = MetropolisHastings(model, step_sizes=step_sizes)

    print(f"\nUsing Metropolis-Hastings sampler")

    chains = sampler.run(
        n_samples=args.n_samples,
        n_chains=args.n_chains,
        burn_in=args.burn_in,
        thin=args.thin,
        seed=args.seed,
    )

    summaries = {}
    for name in ["beta_0", "beta_1", "sigma"]:
        summaries[name] = posterior_summary(chains[name].flatten(), ci_level=args.ci_level)
    print("\n" + format_summary_table(summaries))

    for name in ["beta_0", "beta_1", "sigma"]:
        rhat = compute_rhat(chains, name)
        ess = compute_ess(chains[name].flatten())
        print(f"  {name}: R-hat={rhat:.4f}, ESS={ess:.0f}")
    print(f"  Acceptance rate: {chains['_acceptance_rate'].mean():.3f}")

    if not args.quiet:
        from bayesmcmc.summary import format_trace_ascii, format_histogram_ascii
        print()
        for name in ["beta_0", "beta_1", "sigma"]:
            print(format_trace_ascii(chains[name][0], title=f"{name} (chain 0)"))
            print()
            print(format_histogram_ascii(chains[name].flatten(), title=name))
            print()

    return chains


def run_hierarchical_normal(args):
    """Run the hierarchical normal model."""
    from bayesmcmc.model import Model
    from bayesmcmc.distributions import Normal, Gamma
    from bayesmcmc.samplers import MetropolisHastings
    from bayesmcmc.diagnostics import compute_rhat, compute_ess
    from bayesmcmc.summary import posterior_summary, format_summary_table
    import math

    n_groups = 5
    n_per = 20
    true_mu, true_tau, true_sigma = 5.0, 2.0, 1.0

    rng = np.random.default_rng(args.seed)
    true_thetas = rng.normal(true_mu, true_tau, size=n_groups)
    y = np.array([rng.normal(true_thetas[j], true_sigma, size=n_per) for j in range(n_groups)])

    print("=" * 70)
    print("HIERARCHICAL NORMAL MODEL")
    print("=" * 70)
    print(f"\nTrue: mu={true_mu}, tau={true_tau}, sigma={true_sigma}")
    print(f"Groups: {n_groups}, per group: {n_per}")

    def hier_log_lik(data, **params):
        mu = params["mu"]
        tau = params["tau"]
        sigma = params["sigma"]
        if sigma <= 0 or tau <= 0:
            return -math.inf
        lp = -0.5 * (mu / 10) ** 2
        lp += (2 - 1) * math.log(tau) - 0.5 * tau - math.lgamma(2)
        lp += (2 - 1) * math.log(sigma) - 0.5 * sigma - math.lgamma(2)
        thetas = np.array([params[f"theta_{j}"] for j in range(n_groups)])
        lp += np.sum(-0.5 * ((thetas - mu) / tau) ** 2 - math.log(tau))
        for j in range(n_groups):
            lp += np.sum(-0.5 * ((data[j] - thetas[j]) / sigma) ** 2 - math.log(sigma))
        return lp

    model = Model(name="hierarchical_normal")
    model.add_parameter("mu", Normal(0, 10))
    model.add_parameter("tau", Gamma(2, 0.5))
    model.add_parameter("sigma", Gamma(2, 0.5))
    for j in range(n_groups):
        model.add_parameter(f"theta_{j}", Normal(0, 10))
    model.set_likelihood(hier_log_lik)
    model.set_data(y)

    step_sizes = {"mu": 0.3, "tau": 0.2, "sigma": 0.2}
    for j in range(n_groups):
        step_sizes[f"theta_{j}"] = 0.3

    sampler = MetropolisHastings(model, step_sizes=step_sizes)
    print(f"\nUsing Metropolis-Hastings sampler")

    chains = sampler.run(
        n_samples=args.n_samples,
        n_chains=args.n_chains,
        burn_in=args.burn_in,
        thin=args.thin,
        seed=args.seed,
    )

    param_names = ["mu", "tau", "sigma"] + [f"theta_{j}" for j in range(n_groups)]
    summaries = {}
    for name in param_names:
        summaries[name] = posterior_summary(chains[name].flatten(), ci_level=args.ci_level)
    print("\n" + format_summary_table(summaries))

    print(f"\n  Acceptance rate: {chains['_acceptance_rate'].mean():.3f}")

    if not args.quiet:
        from bayesmcmc.summary import format_trace_ascii, format_histogram_ascii
        print()
        for name in ["mu", "tau", "sigma"]:
            print(format_trace_ascii(chains[name][0], title=f"{name} (chain 0)"))
            print()
            print(format_histogram_ascii(chains[name].flatten(), title=name))
            print()

    return chains


def main(argv=None):
    args = parse_args(argv)

    if args.model == "beta_binomial":
        run_beta_binomial(args)
    elif args.model == "linear_regression":
        run_linear_regression(args)
    elif args.model == "hierarchical_normal":
        run_hierarchical_normal(args)
    else:
        print(f"Unknown model: {args.model}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
