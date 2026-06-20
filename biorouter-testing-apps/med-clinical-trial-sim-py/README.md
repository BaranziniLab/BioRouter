# Med Clinical Trial Simulator

An adaptive clinical-trial design simulator in pure Python.

## Features

- **Two-arm and multi-arm trials** with configurable effect sizes, accrual, and dropout
- **Fixed designs** with automatic sample-size calculation
- **Group-sequential designs** with O'Brien-Fleming / Pocock alpha-spending and interim analyses
  - Efficacy and futility stopping rules
  - Information-fraction based monitoring
- **Response-adaptive randomisation** (Bayesian allocation, Thompson sampling)
- **Outcome models**: binary, continuous, time-to-event (exponential)
- **Operating characteristics** via Monte Carlo simulation
  - Type-I error, power, expected sample size, stopping probabilities
- **CLI and OC table** for running designs across scenarios

## Quick Start

```bash
# Install in development mode
pip install -e ".[dev]"

# Run a fixed-design trial
python -m med_clinical_trial_sim --design fixed --outcome binary \
    --p-control 0.3 --p-treatment 0.5 --n-per-arm 100 --alpha 0.05

# Group-sequential with O'Brien-Fleming spending
python -m med_clinical_trial_sim --design group_sequential \
    --outcome binary --p-control 0.3 --p-treatment 0.5 \
    --n-analyses 5 --spending obrien_fleming --alpha 0.05

# Response-adaptive (Bayesian allocation)
python -m med_clinical_trial_sim --design response_adaptive \
    --outcome binary --p-control 0.3 --p-treatment 0.5 \
    --n-max 200 --allocation bayesian

# Effect-size sweep
python -m med_clinical_trial_sim --design fixed --outcome binary \
    --p-control 0.3 --n-per-arm 100 --n-reps 2000 --sweep-effect
```

## Project Structure

```
src/med_clinical_trial_sim/
├── __init__.py
├── __main__.py          # Entry point
├── outcomes.py          # Outcome models (binary, continuous, TTE)
├── spending.py          # Alpha-spending functions (OBF, Pocock)
├── simulate.py          # Monte Carlo simulation engine
├── oc.py                # Operating characteristics table
├── cli.py               # Command-line interface
└── designs/
    ├── __init__.py
    ├── fixed.py             # Fixed sample-size design
    ├── group_sequential.py  # Group-sequential design
    └── response_adaptive.py # Response-adaptive randomisation
```

## Running Tests

```bash
pytest
```

## Dependencies

- Python ≥ 3.9
- Optional: numpy, scipy (for faster random number generation)
- Dev: pytest

## Mathematical Background

### Alpha-Spending (Lan-DeMets)

The Lan-DeMets framework specifies cumulative Type-I error spending as a function of information fraction *t*:

- **O'Brien-Fleming**: α*(t) = 2 − 2·Φ(z_{α/2} / √t) — conservative early, aggressive at final
- **Pocock**: α*(t) = α · ln(1 + (e−1)·t) — more uniform spending

### Response-Adaptive Randomisation

Bayesian allocation computes posterior means and assigns allocation probability proportional to estimated benefit, with a floor to ensure each arm continues to be explored.

### Time-to-Event

Uses exponential survival with Schoenfeld sample-size formula and log-rank test statistic.
