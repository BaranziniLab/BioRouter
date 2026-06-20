"""
Beta-Binomial model example.

This is the classic conjugate Bayesian example:
- Prior: p ~ Beta(alpha, beta)
- Likelihood: y | p ~ Binomial(n, p)
- Posterior: p | y ~ Beta(alpha + k, beta + n - k)

We demonstrate:
1. Analytic conjugate posterior
2. MH sampling
3. Gibbs sampling with Beta full conditional
4. Slice sampling
5. Comparison of posterior estimates
"""

import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

import numpy as np
from bayesmcmc.model import Model
from bayesmcmc.distributions import Beta
from bayesmcmc.samplers import MetropolisHastings, GibbsSampler, SliceSampler
from bayesmcmc.diagnostics import compute_rhat, compute_ess, trace_summary
from bayesmcmc.summary import posterior_summary, format_summary_table


def main():
    # observed data: 7 heads in 10 trials
    data = np.array([1, 1, 1, 1, 1, 1, 1, 0, 0, 0])

    # --- Analytic posterior ---
    alpha_prior, beta_prior = 1.0, 1.0
    k = data.sum()
    n = len(data)
    alpha_post = alpha_prior + k
    beta_post = beta_prior + n - k

    print("=" * 70)
    print("BETA-BINOMIAL MODEL")
    print("=" * 70)
    print(f"\nData: {k} successes in {n} trials")
    print(f"Prior: Beta({alpha_prior}, {beta_prior})")
    print(f"Analytic Posterior: Beta({alpha_post}, {beta_post})")
    print(f"Analytic Mean: {alpha_post / (alpha_post + beta_post):.4f}")
    print(f"Analytic Variance: {alpha_post * beta_post / ((alpha_post + beta_post)**2 * (alpha_post + beta_post + 1)):.6f}")

    # --- Build model ---
    model = Model.beta_binomial(alpha_prior, beta_prior)
    model.set_data(data)

    n_samples = 5000
    n_chains = 4
    seed = 42

    # --- Metropolis-Hastings ---
    print("\n" + "-" * 70)
    print("Metropolis-Hastings Sampler")
    mh_sampler = MetropolisHastings(model, step_sizes={"p": 0.1})
    mh_chains = mh_sampler.run(
        n_samples=n_samples, n_chains=n_chains, burn_in=1000, seed=seed
    )
    mh_summary = posterior_summary(mh_chains["p"].flatten())
    print(format_summary_table({"p (MH)": mh_summary}))
    print(f"  R-hat: {compute_rhat(mh_chains, 'p'):.4f}")
    print(f"  ESS: {compute_ess(mh_chains['p'].flatten()):.0f}")

    # --- Gibbs sampling ---
    print("\n" + "-" * 70)
    print("Gibbs Sampler (Beta full conditional)")
    full_cond = GibbsSampler.beta_binomial_conditionals(alpha_prior, beta_prior)
    gibbs_sampler = GibbsSampler(model, full_conditionals=full_cond)
    gibbs_chains = gibbs_sampler.run(
        n_samples=n_samples, n_chains=n_chains, burn_in=1000, seed=seed
    )
    gibbs_summary = posterior_summary(gibbs_chains["p"].flatten())
    print(format_summary_table({"p (Gibbs)": gibbs_summary}))
    print(f"  R-hat: {compute_rhat(gibbs_chains, 'p'):.4f}")
    print(f"  ESS: {compute_ess(gibbs_chains['p'].flatten()):.0f}")

    # --- Slice sampling ---
    print("\n" + "-" * 70)
    print("Slice Sampler")
    slice_sampler = SliceSampler(model, width=0.3)
    slice_chains = slice_sampler.run(
        n_samples=n_samples, n_chains=n_chains, burn_in=1000, seed=seed
    )
    slice_summary = posterior_summary(slice_chains["p"].flatten())
    print(format_summary_table({"p (Slice)": slice_summary}))
    print(f"  R-hat: {compute_rhat(slice_chains, 'p'):.4f}")
    print(f"  ESS: {compute_ess(slice_chains['p'].flatten()):.0f}")

    # --- Comparison ---
    print("\n" + "=" * 70)
    print("COMPARISON")
    print("=" * 70)
    print(f"  Analytic mean:  {alpha_post / (alpha_post + beta_post):.4f}")
    print(f"  MH mean:        {mh_summary['mean']:.4f}")
    print(f"  Gibbs mean:     {gibbs_summary['mean']:.4f}")
    print(f"  Slice mean:     {slice_summary['mean']:.4f}")


if __name__ == "__main__":
    main()
