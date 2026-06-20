# med-epidemic-seir-model-py

Epidemic compartmental modeling toolkit in pure Python + NumPy.

## Models

| Model | States | Key Parameters |
|-------|--------|---------------|
| **SIR** | S → I → R | β (transmission), γ (recovery) |
| **SEIR** | S → E → I → R | β, σ (incubation), γ |
| **SEIRD** | S → E → I → R/D | β, σ, γ, μ (mortality) |
| **SEIR + Interventions** | S → E → I → R | β(t) with time-varying lockdowns/NPIs |

## Features

- **Deterministic ODE solver** — configurable RK4 with fixed step
- **Stochastic simulation** — Gillespie SSA for SIR, SEIR, SEIRD (small populations)
- **Epidemic metrics** — R₀, effective Rₜ over time, peak infections + timing, attack rate, final size
- **Parameter fitting** — grid search + least-squares refinement on (β, σ, γ)
- **Scenario comparison** — compare intervention vs. no-intervention scenarios
- **CLI** — run any model with parameters, print metrics, ASCII plot, export CSV
- **ASCII plots** — terminal-friendly compartment visualizations

## Installation

```bash
pip install -e ".[dev]"
```

## Usage

```bash
# SIR model with default parameters
med-epidemic sir

# SEIR with custom parameters
med-epidemic seir --beta 0.3 --sigma 0.2 --gamma 0.1 --N 10000 --I0 10

# SEIRD (with deaths)
med-epidemic seird --mu 0.01

# SEIR with lockdown intervention
med-epidemic seir-intervention --beta 0.4 --lockdown-start 30 --lockdown-reduction 0.7

# Stochastic SIR (Gillespie)
med-epidemic stochastic-sir --N 500 --beta 0.5 --gamma 0.2

# Fit to observed data
med-epidemic fit --data cases.csv --model seir --N 10000

# Export trajectory to CSV
med-epidemic sir --export-csv trajectory.csv
```

## Project Structure

```
src/med_epidemic/
├── __init__.py
├── solver.py           # RK4 ODE solver
├── models/
│   ├── __init__.py
│   ├── sir.py          # SIR model
│   ├── seir.py         # SEIR model
│   ├── seird.py        # SEIRD model
│   └── seir_intervention.py  # SEIR with time-varying β
├── stochastic.py       # Gillespie SSA
├── metrics.py          # Epidemic summary metrics
├── fit.py              # Parameter fitting
├── plot_ascii.py       # ASCII plot renderer
└── cli.py              # Command-line interface

tests/
├── test_solver.py
├── test_models.py
├── test_stochastic.py
├── test_metrics.py
├── test_fit.py
└── test_cli.py
```

## Testing

```bash
pytest
```

## License

MIT
