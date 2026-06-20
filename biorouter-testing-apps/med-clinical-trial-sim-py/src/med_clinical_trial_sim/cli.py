"""
Command-line interface for the clinical trial simulator.

Usage
-----
    python -m med_clinical_trial_sim [OPTIONS]

Examples
--------
    # Fixed design, binary endpoint
    python -m med_clinical_trial_sim --design fixed --outcome binary \\
        --p-control 0.3 --p-treatment 0.5 --n-per-arm 100 --alpha 0.05

    # Group-sequential with O'Brien-Fleming spending
    python -m med_clinical_trial_sim --design group_sequential \\
        --outcome binary --p-control 0.3 --p-treatment 0.5 \\
        --n-analyses 5 --spending obrien_fleming --alpha 0.05

    # Response-adaptive (Bayesian allocation)
    python -m med_clinical_trial_sim --design response_adaptive \\
        --outcome binary --p-control 0.3 --p-treatment 0.5 \\
        --n-max 200 --allocation bayesian
"""

from __future__ import annotations

import argparse
import sys
from typing import List, Optional

from .oc import OCTable, build_oc_table
from .outcomes import BinaryOutcome, ContinuousOutcome, TimeToEventOutcome, OutcomeModel
from .spending import OBrienFleming, Pocock
from .designs.fixed import FixedDesign
from .designs.group_sequential import GroupSequentialDesign
from .designs.response_adaptive import ResponseAdaptiveDesign
from .simulate import run_simulation


# ---------------------------------------------------------------------------
# Argument parser
# ---------------------------------------------------------------------------

def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="trial-sim",
        description="Adaptive clinical trial design simulator",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )

    # Design
    p.add_argument("--design", choices=["fixed", "group_sequential", "response_adaptive"],
                   default="fixed", help="Trial design type (default: fixed)")
    p.add_argument("--outcome", choices=["binary", "continuous", "tte"],
                   default="binary", help="Outcome type (default: binary)")

    # Effect sizes — binary
    p.add_argument("--p-control", type=float, default=0.30,
                   help="Control arm probability (binary, default 0.30)")
    p.add_argument("--p-treatment", type=float, default=0.50,
                   help="Treatment arm probability (binary, default 0.50)")

    # Effect sizes — continuous
    p.add_argument("--mean-control", type=float, default=0.0,
                   help="Control arm mean (continuous)")
    p.add_argument("--mean-treatment", type=float, default=0.5,
                   help="Treatment arm mean (continuous)")
    p.add_argument("--std-dev", type=float, default=1.0,
                   help="Common std dev (continuous)")

    # Effect sizes — TTE
    p.add_argument("--median-control", type=float, default=12.0,
                   help="Control median survival (TTE)")
    p.add_argument("--hazard-ratio", type=float, default=0.65,
                   help="Hazard ratio (TTE)")
    p.add_argument("--median-censor", type=float, default=24.0,
                   help="Administrative censoring median (TTE)")

    # Sample size
    p.add_argument("--n-per-arm", type=int, default=None,
                   help="Fixed or max sample size per arm")
    p.add_argument("--n-max", type=int, default=200,
                   help="Max per-arm for response-adaptive (default 200)")
    p.add_argument("--power", type=float, default=0.80,
                   help="Target power for sample-size calculation")
    p.add_argument("--dropout-rate", type=float, default=0.0,
                   help="Dropout rate")

    # Group-sequential
    p.add_argument("--n-analyses", type=int, default=5,
                   help="Number of analyses (group-sequential, default 5)")
    p.add_argument("--spending", choices=["obrien_fleming", "pocock"],
                   default="obrien_fleming", help="Alpha-spending function")
    p.add_argument("--futiltiy", action="store_true", default=True,
                   help="Enable futiltiy stopping (default True)")
    p.add_argument("--no-futiltiy", dest="futiltiy", action="store_false",
                   help="Disable futiltiy stopping")

    # Response-adaptive
    p.add_argument("--allocation", choices=["bayesian", "thompson"],
                   default="bayesian", help="Allocation rule")
    p.add_argument("--block-size", type=int, default=5,
                   help="Block size for response-adaptive")
    p.add_argument("--efficacy-bound", type=float, default=None,
                   help="Z-boundary for early efficacy (response-adaptive)")

    # Common
    p.add_argument("--alpha", type=float, default=0.05,
                   help="Two-sided significance level (default 0.05)")
    p.add_argument("--n-reps", type=int, default=1000,
                   help="Monte Carlo replicates (default 1000)")
    p.add_argument("--seed", type=int, default=None,
                   help="Random seed for reproducibility")
    p.add_argument("--verbose", action="store_true", default=False,
                   help="Print progress during simulation")

    # Scenario sweep
    p.add_argument("--sweep-effect", action="store_true", default=False,
                   help="Run a sweep over multiple effect sizes")

    return p


# ---------------------------------------------------------------------------
# Build outcome and design from args
# ---------------------------------------------------------------------------

def _make_outcome(args: argparse.Namespace) -> OutcomeModel:
    if args.outcome == "binary":
        return BinaryOutcome(p_control=args.p_control, p_treatment=args.p_treatment)
    elif args.outcome == "continuous":
        return ContinuousOutcome(mean_control=args.mean_control, std_dev=args.std_dev,
                                  mean_treatment=args.mean_treatment)
    elif args.outcome == "tte":
        return TimeToEventOutcome(median_control=args.median_control,
                                   hazard_ratio=args.hazard_ratio,
                                   median_censor=args.median_censor)
    raise ValueError(f"Unknown outcome: {args.outcome}")


def _make_design(args: argparse.Namespace):
    outcome = _make_outcome(args)

    if args.design == "fixed":
        return FixedDesign(
            outcome=outcome,
            n_per_arm=args.n_per_arm,
            alpha=args.alpha,
            power=args.power,
            dropout_rate=args.dropout_rate,
        )
    elif args.design == "group_sequential":
        spending_fn = OBrienFleming() if args.spending == "obrien_fleming" else Pocock()
        return GroupSequentialDesign(
            outcome=outcome,
            n_per_arm=args.n_per_arm,
            n_analyses=args.n_analyses,
            alpha=args.alpha,
            power=args.power,
            spending=spending_fn,
            futiltiy=args.futiltiy,
            dropout_rate=args.dropout_rate,
        )
    elif args.design == "response_adaptive":
        return ResponseAdaptiveDesign(
            outcome=outcome,
            n_max=args.n_max,
            alpha=args.alpha,
            allocation=args.allocation,
            block_size=args.block_size,
            efficacy_bound=args.efficacy_bound,
        )
    raise ValueError(f"Unknown design: {args.design}")


# ---------------------------------------------------------------------------
# Effect-size sweep
# ---------------------------------------------------------------------------

def _sweep_effect(args: argparse.Namespace) -> List:
    """Run a sweep over multiple effect sizes and return (label, sim) pairs."""
    pairs = []

    if args.outcome == "binary":
        # Sweep p_treatment from p_control (null) to 0.7
        base_p_ctrl = args.p_control
        for pt in [base_p_ctrl, base_p_ctrl + 0.05, base_p_ctrl + 0.10,
                   base_p_ctrl + 0.15, base_p_ctrl + 0.20, base_p_ctrl + 0.25]:
            pt = min(pt, 1.0)
            args_copy = argparse.Namespace(**vars(args))
            args_copy.p_treatment = pt
            design = _make_design(args_copy)
            label = f"p_ctrl={base_p_ctrl}, p_treat={pt} (Δ={pt - base_p_ctrl:.2f})"
            sim = run_simulation(design, n_reps=args.n_reps, seed=args.seed,
                                 verbose=args.verbose)
            pairs.append((label, sim))
    elif args.outcome == "continuous":
        base_mu = args.mean_control
        for mu_t in [base_mu, base_mu + 0.2, base_mu + 0.4, base_mu + 0.6,
                     base_mu + 0.8, base_mu + 1.0]:
            args_copy = argparse.Namespace(**vars(args))
            args_copy.mean_treatment = mu_t
            design = _make_design(args_copy)
            label = f"μ_ctrl={base_mu}, μ_treat={mu_t} (δ={mu_t - base_mu:.1f})"
            sim = run_simulation(design, n_reps=args.n_reps, seed=args.seed,
                                 verbose=args.verbose)
            pairs.append((label, sim))
    elif args.outcome == "tte":
        for hr in [1.0, 0.85, 0.75, 0.65, 0.55, 0.45]:
            args_copy = argparse.Namespace(**vars(args))
            args_copy.hazard_ratio = hr
            design = _make_design(args_copy)
            label = f"HR={hr}"
            sim = run_simulation(design, n_reps=args.n_reps, seed=args.seed,
                                 verbose=args.verbose)
            pairs.append((label, sim))

    return pairs


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main(argv: Optional[List[str]] = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    print("=" * 70)
    print("  Clinical Trial Simulator")
    print("=" * 70)
    print(f"  Design:     {args.design}")
    print(f"  Outcome:    {args.outcome}")
    print(f"  Alpha:      {args.alpha}")
    print(f"  Replicates: {args.n_reps}")
    if args.seed is not None:
        print(f"  Seed:       {args.seed}")
    print()

    if args.sweep_effect:
        print("Running effect-size sweep...")
        pairs = _sweep_effect(args)
    else:
        design = _make_design(args)
        print(f"  Design: {design}")
        print()
        print("Running simulation...")
        sim = run_simulation(design, n_reps=args.n_reps, seed=args.seed,
                             verbose=args.verbose)
        summary = sim.summary()
        print()
        print("Operating Characteristics:")
        for k, v in summary.items():
            print(f"  {k:30s}: {v}")
        print()
        pairs = [("Single scenario", sim)]

    # Build and display OC table
    table = build_oc_table(pairs)
    print()
    print(table.format_table())

    return 0


if __name__ == "__main__":
    sys.exit(main())
