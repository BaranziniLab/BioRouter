"""Command-line interface for med-epidemic-seir-model-py.

Usage examples::

    # Run SIR with default parameters
    med-epidemic sir

    # Run SEIR with custom parameters
    med-epidemic seir --beta 0.3 --sigma 0.2 --gamma 0.1 --N 10000 --I0 10

    # Run SEIRD
    med-epidemic seird --mu 0.01

    # Run SEIR with interventions
    med-epidemic seir-intervention --beta 0.4 --lockdown-start 30 --lockdown-reduction 0.7

    # Run stochastic SIR
    med-epidemic stochastic-sir --N 500 --beta 0.5 --gamma 0.2

    # Fit to CSV data
    med-epidemic fit --data cases.csv --model seir --N 10000
"""

from __future__ import annotations

import argparse
import csv
import sys
from pathlib import Path
from typing import List, Optional

import numpy as np

from med_epidemic.metrics import compute_metrics, compute_Rt
from med_epidemic.plot_ascii import ascii_plot


def _add_common_args(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--N", type=float, default=10000, help="Total population")
    parser.add_argument("--beta", type=float, default=0.3, help="Transmission rate")
    parser.add_argument("--gamma", type=float, default=0.1, help="Recovery rate")
    parser.add_argument("--I0", type=float, default=1.0, help="Initial infected")
    parser.add_argument("--t-max", type=float, default=160, help="Simulation end time (days)")
    parser.add_argument("--dt", type=float, default=0.5, help="ODE step size")
    parser.add_argument("--no-plot", action="store_true", help="Suppress ASCII plot")
    parser.add_argument("--export-csv", type=str, default=None, help="Export trajectory to CSV")
    parser.add_argument("--quiet", action="store_true", help="Suppress plot and metrics output")


def cmd_sir(args: argparse.Namespace) -> None:
    from med_epidemic.models.sir import SIRModel, SIRParams

    params = SIRParams(beta=args.beta, gamma=args.gamma, N=args.N, I0=args.I0)
    model = SIRModel(params)
    sol = model.run(t_span=(0, args.t_max), dt=args.dt)

    names = model.state_names()
    metrics = compute_metrics(sol, args.beta, args.gamma, args.N, s_index=0, i_index=1, r_index=2)

    if not args.quiet:
        print(f"\n{'='*60}")
        print(f"  SIR Model Results  (R₀ = {metrics.R0:.2f})")
        print(f"{'='*60}")
        for k, v in metrics.summary_dict().items():
            print(f"  {k:.<35s} {v}")
        print(f"{'='*60}\n")

        if not args.no_plot:
            print(ascii_plot(sol.t, [sol[0], sol[1], sol[2]], names,
                             title="SIR Model"))

    _maybe_export(args.export_csv, sol.t, sol.y, names)


def cmd_seir(args: argparse.Namespace) -> None:
    from med_epidemic.models.seir import SEIRModel, SEIRParams

    sigma = getattr(args, "sigma", 0.2)
    params = SEIRParams(beta=args.beta, sigma=sigma, gamma=args.gamma,
                        N=args.N, I0=args.I0)
    model = SEIRModel(params)
    sol = model.run(t_span=(0, args.t_max), dt=args.dt)

    names = model.state_names()
    metrics = compute_metrics(sol, args.beta, args.gamma, args.N, s_index=0, i_index=2, r_index=3)

    if not args.quiet:
        print(f"\n{'='*60}")
        print(f"  SEIR Model Results  (R₀ = {metrics.R0:.2f})")
        print(f"{'='*60}")
        for k, v in metrics.summary_dict().items():
            print(f"  {k:.<35s} {v}")
        print(f"{'='*60}\n")

        if not args.no_plot:
            print(ascii_plot(sol.t, [sol[0], sol[1], sol[2], sol[3]], names,
                             title="SEIR Model"))

    _maybe_export(args.export_csv, sol.t, sol.y, names)


def cmd_seird(args: argparse.Namespace) -> None:
    from med_epidemic.models.seird import SEIRDModel, SEIRDParams

    sigma = getattr(args, "sigma", 0.2)
    mu = getattr(args, "mu", 0.01)
    params = SEIRDParams(beta=args.beta, sigma=sigma, gamma=args.gamma,
                         mu=mu, N=args.N, I0=args.I0)
    model = SEIRDModel(params)
    sol = model.run(t_span=(0, args.t_max), dt=args.dt)

    names = model.state_names()
    metrics = compute_metrics(sol, args.beta, args.gamma, args.N, s_index=0, i_index=2, r_index=3)

    if not args.quiet:
        print(f"\n{'='*60}")
        print(f"  SEIRD Model Results  (R₀ = {metrics.R0:.2f})")
        print(f"{'='*60}")
        for k, v in metrics.summary_dict().items():
            print(f"  {k:.<35s} {v}")
        print(f"{'='*60}\n")

        if not args.no_plot:
            print(ascii_plot(sol.t, [sol[0], sol[1], sol[2], sol[3], sol[4]], names,
                             title="SEIRD Model"))

    _maybe_export(args.export_csv, sol.t, sol.y, names)


def cmd_seir_intervention(args: argparse.Namespace) -> None:
    from med_epidemic.models.seir_intervention import (
        SEIRInterventionModel, SEIRInterventionParams, Intervention,
    )

    sigma = getattr(args, "sigma", 0.2)
    ivs = []
    if getattr(args, "lockdown_start", None) is not None:
        ivs.append(Intervention(
            start=args.lockdown_start,
            end=getattr(args, "lockdown_end", None),
            reduction=getattr(args, "lockdown_reduction", 0.5),
        ))
    params = SEIRInterventionParams(
        beta_base=args.beta, sigma=sigma, gamma=args.gamma,
        N=args.N, I0=args.I0, interventions=ivs,
    )
    model = SEIRInterventionModel(params)
    sol = model.run(t_span=(0, args.t_max), dt=args.dt)

    names = model.state_names()
    metrics = compute_metrics(sol, args.beta, args.gamma, args.N, s_index=0, i_index=2, r_index=3)

    if not args.quiet:
        print(f"\n{'='*60}")
        print(f"  SEIR + Intervention Model Results  (R₀ = {metrics.R0:.2f})")
        print(f"{'='*60}")
        for k, v in metrics.summary_dict().items():
            print(f"  {k:.<35s} {v}")
        print(f"{'='*60}\n")

        if not args.no_plot:
            print(ascii_plot(sol.t, [sol[0], sol[1], sol[2], sol[3]], names,
                             title="SEIR + Intervention"))

    _maybe_export(args.export_csv, sol.t, sol.y, names)


def cmd_stochastic_sir(args: argparse.Namespace) -> None:
    from med_epidemic.stochastic import run_sir_gillespie

    N = int(args.N)
    I0 = int(args.I0)
    t, y = run_sir_gillespie(N=N, beta=args.beta, gamma=args.gamma, I0=I0,
                              t_span=(0, args.t_max))

    names = ("S", "I", "R")

    if not args.quiet:
        print(f"\n{'='*60}")
        print(f"  Stochastic SIR (Gillespie SSA)")
        print(f"  N={N}, β={args.beta}, γ={args.gamma}, I₀={I0}")
        print(f"{'='*60}")
        print(f"  Final S: {y[0, -1]}, I: {y[1, -1]}, R: {y[2, -1]}")
        print(f"  Events: {len(t)}")
        print(f"{'='*60}\n")

        if not args.no_plot:
            print(ascii_plot(t, [y[0].astype(float), y[1].astype(float), y[2].astype(float)],
                             names, title="Stochastic SIR"))

    _maybe_export(args.export_csv, t, y.astype(float), names)


def cmd_fit(args: argparse.Namespace) -> None:
    """Fit model parameters to observed data from a CSV."""
    from med_epidemic.fit import fit_seir, fit_sir

    csv_path = Path(args.data)
    if not csv_path.exists():
        print(f"Error: {csv_path} not found", file=sys.stderr)
        sys.exit(1)

    # read CSV: expected columns "time" and "infected"
    times, infected = [], []
    with open(csv_path) as f:
        reader = csv.DictReader(f)
        for row in reader:
            times.append(float(row["time"]))
            infected.append(float(row["infected"]))

    t_obs = np.array(times)
    I_obs = np.array(infected)
    N = args.N
    model_type = getattr(args, "model", "seir")

    if model_type == "sir":
        params = fit_sir(t_obs, I_obs, N)
    else:
        params = fit_seir(t_obs, I_obs, N)

    print(f"\nFitted {model_type.upper()} parameters:")
    for k, v in params.items():
        print(f"  {k}: {v:.6f}")

    # Run with fitted params and show fit quality
    if model_type == "sir":
        from med_epidemic.models.sir import SIRModel, SIRParams
        p = SIRParams(beta=params["beta"], gamma=params["gamma"], N=N, I0=I_obs[0])
        model = SIRModel(p)
        sol = model.run(t_span=(t_obs[0], t_obs[-1]), dt=0.5)
        I_fit = np.interp(t_obs, sol.t, sol.y[1])
    else:
        from med_epidemic.models.seir import SEIRModel, SEIRParams
        p = SEIRParams(
            beta=params["beta"], sigma=params.get("sigma", 0.2),
            gamma=params["gamma"], N=N, I0=I_obs[0],
        )
        model = SEIRModel(p)
        sol = model.run(t_span=(t_obs[0], t_obs[-1]), dt=0.5)
        I_fit = np.interp(t_obs, sol.t, sol.y[2])

    rmse = float(np.sqrt(np.mean((I_obs - I_fit) ** 2)))
    print(f"  RMSE: {rmse:.2f}")

    if not args.no_plot:
        print()
        print(ascii_plot(t_obs, [I_obs, I_fit], ["Observed", "Fitted"],
                         title=f"{model_type.upper()} Fit"))


def _maybe_export(path: Optional[str], t: np.ndarray, y: np.ndarray, names: tuple) -> None:
    """Export trajectory to CSV if path is given."""
    if path is None:
        return
    with open(path, "w", newline="") as f:
        writer = csv.writer(f)
        header = ["time"] + list(names)
        writer.writerow(header)
        for i in range(len(t)):
            row = [t[i]] + [y[s, i] for s in range(len(names))]
            writer.writerow(row)
    print(f"Trajectory exported to {path}")


# ---------------------------------------------------------------------------
# Argument parser
# ---------------------------------------------------------------------------

def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="med-epidemic",
        description="Epidemic compartmental modeling toolkit",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    # --- sir ---
    p_sir = sub.add_parser("sir", help="Run SIR model")
    _add_common_args(p_sir)
    p_sir.set_defaults(func=cmd_sir)

    # --- seir ---
    p_seir = sub.add_parser("seir", help="Run SEIR model")
    _add_common_args(p_seir)
    p_seir.add_argument("--sigma", type=float, default=0.2, help="Incubation rate")
    p_seir.set_defaults(func=cmd_seir)

    # --- seird ---
    p_seird = sub.add_parser("seird", help="Run SEIRD model")
    _add_common_args(p_seird)
    p_seird.add_argument("--sigma", type=float, default=0.2, help="Incubation rate")
    p_seird.add_argument("--mu", type=float, default=0.01, help="Mortality rate")
    p_seird.set_defaults(func=cmd_seird)

    # --- seir-intervention ---
    p_siri = sub.add_parser("seir-intervention", help="SEIR with interventions")
    _add_common_args(p_siri)
    p_siri.add_argument("--sigma", type=float, default=0.2, help="Incubation rate")
    p_siri.add_argument("--lockdown-start", type=float, default=None, help="Lockdown start day")
    p_siri.add_argument("--lockdown-end", type=float, default=None, help="Lockdown end day")
    p_siri.add_argument("--lockdown-reduction", type=float, default=0.5, help="Transmission reduction (0-1)")
    p_siri.set_defaults(func=cmd_seir_intervention)

    # --- stochastic-sir ---
    p_ssir = sub.add_parser("stochastic-sir", help="Stochastic SIR (Gillespie)")
    _add_common_args(p_ssir)
    p_ssir.set_defaults(func=cmd_stochastic_sir)

    # --- fit ---
    p_fit = sub.add_parser("fit", help="Fit model to observed data")
    p_fit.add_argument("--data", type=str, required=True, help="CSV with 'time','infected' columns")
    p_fit.add_argument("--model", type=str, default="seir", choices=["sir", "seir"])
    p_fit.add_argument("--N", type=float, default=10000, help="Total population")
    p_fit.add_argument("--no-plot", action="store_true")
    p_fit.set_defaults(func=cmd_fit)

    return parser


def main(argv: Optional[List[str]] = None) -> None:
    parser = build_parser()
    args = parser.parse_args(argv)
    args.func(args)


if __name__ == "__main__":
    main()
