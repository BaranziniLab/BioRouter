"""
bayesmcmc - Bayesian inference and MCMC library.

A pure-Python (+ optional NumPy) library providing:
- MCMC samplers (Metropolis-Hastings, Gibbs, HMC, Slice)
- Model specification API
- Conjugate updates
- MCMC diagnostics
- Posterior summaries
"""

__version__ = "0.1.0"
__author__ = "bayesmcmc contributors"

from bayesmcmc.distributions import (
    Normal,
    MultivariateNormal,
    Bernoulli,
    Binomial,
    Poisson,
    Gamma,
    Beta,
    Uniform,
    StudentT,
)
from bayesmcmc.model import Model
from bayesmcmc.samplers import (
    MetropolisHastings,
    GibbsSampler,
    HMCSampler,
    SliceSampler,
)
from bayesmcmc.diagnostics import (
    compute_ess,
    compute_rhat,
    autocorrelation,
    trace_summary,
    geweke_diagnostic,
)
from bayesmcmc.summary import (
    posterior_mean,
    posterior_median,
    credible_interval,
    hpd_interval,
    posterior_summary,
    multi_param_summary,
)

__all__ = [
    "Normal", "MultivariateNormal", "Bernoulli", "Binomial",
    "Poisson", "Gamma", "Beta", "Uniform", "StudentT",
    "Model",
    "MetropolisHastings", "GibbsSampler", "HMCSampler", "SliceSampler",
    "compute_ess", "compute_rhat", "autocorrelation",
    "trace_summary", "geweke_diagnostic",
    "posterior_mean", "posterior_median", "credible_interval",
    "hpd_interval", "posterior_summary", "multi_param_summary",
]
