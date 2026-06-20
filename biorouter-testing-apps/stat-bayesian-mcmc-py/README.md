# bayesmcmc

A pure-Python (+ optional NumPy) Bayesian inference and MCMC library.

## Features

- **Samplers**: Metropolis-Hastings (random-walk + adaptive), Gibbs, Hamiltonian Monte Carlo, Slice sampling
- **Distributions**: Normal, Bernoulli, Binomial, Poisson, Gamma, Beta with log-pdf support
- **Model API**: Compose models from common distributions or define custom log-prior + log-likelihood
- **Conjugate Updates**: Analytic posterior updates for conjugate pairs where available
- **Diagnostics**: ESS, Gelman-Rubin R-hat, autocorrelation, acceptance rate, burn-in/thinning
- **Posterior Summaries**: Mean, credible intervals, HPD intervals, quantiles
- **Worked Examples**: Bayesian linear regression, beta-binomial, hierarchical normal
- **CLI**: Run models and print ASCII trace plots, histograms, and diagnostics

## Installation

```bash
pip install -e ".[dev]"
```

## Usage

### CLI

```bash
# Run beta-binomial example
python -m bayesmcmc.cli --model beta_binomial --data "7,10"

# Run Bayesian linear regression
python -m bayesmcmc.cli --model linear_regression
```

### Python API

```python
from bayesmcmc.model import Model
from bayesmcmc.distributions import Normal, Beta
from bayesmcmc.samplers import MetropolisHastings

# Define a model
model = Model()
model.add_parameter("mu", Normal(mu=0, sigma=10))
model.add_parameter("sigma", Beta(a=2, b=2))

# Run MCMC
sampler = MetropolisHastings(model)
samples = sampler.run(n_samples=1000, n_chains=4)

# Diagnostics
from bayesmcmc.diagnostics import compute_rhat, compute_ess
print(f"R-hat: {compute_rhat(samples)}")
print(f"ESS: {compute_ess(samples)}")
```

## Project Structure

```
src/bayesmcmc/
    __init__.py
    distributions.py      # Probability distributions
    model.py              # Model specification API
    diagnostics.py        # MCMC diagnostics
    summary.py            # Posterior summaries
    cli.py                # CLI driver
    samplers/
        __init__.py
        mh.py             # Metropolis-Hastings
        gibbs.py          # Gibbs sampling
        hmc.py            # Hamiltonian Monte Carlo
        slice.py          # Slice sampling
examples/
    bayesian_linear_regression.py
    beta_binomial.py
    hierarchical_normal.py
tests/
    test_distributions.py
    test_model.py
    test_samplers.py
    test_diagnostics.py
    test_summary.py
    test_examples.py
    test_cli.py
```

## License

MIT
