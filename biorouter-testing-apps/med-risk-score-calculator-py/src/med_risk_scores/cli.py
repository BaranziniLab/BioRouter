"""
Command-line interface for the clinical risk-score calculator.

Usage::

    # List available scores
    med-risk-score list

    # Compute a score
    med-risk-score compute cha2ds2_vasc --chf 0 --hypertension 1 --age 72 \
        --diabetes 1 --stroke-tia 0 --vascular-disease 0 --sex-female 1

    # Show score details
    med-risk-score info cha2ds2_vasc

    # Compute from JSON stdin
    echo '{"chf":false,"hypertension":true,"age":72,...}' | med-risk-score compute cha2ds2_vasc --json
"""
from __future__ import annotations

import argparse
import json
import sys
from typing import List

from med_risk_scores.engine import compute, compute_safe
from med_risk_scores.registry import all_definitions, get_score, list_scores
from med_risk_scores.validate import ValidationException


def _add_compute_args(parser: argparse.ArgumentParser, defn) -> None:
    """Add --flag arguments for each variable in the score definition."""
    for var in defn.variables:
        flag = f"--{var.name.replace('_', '-')}"
        kwargs = {"help": var.description}
        if var.var_type == "boolean":
            kwargs["type"] = lambda x: x.lower() in ("1", "true", "yes")
            kwargs["default"] = None
        elif var.var_type == "enum":
            kwargs["type"] = str
            kwargs["choices"] = list(var.allowed_values or [])
            kwargs["default"] = None
        else:
            kwargs["type"] = float
            kwargs["default"] = None
        if var.unit:
            kwargs["help"] += f" ({var.unit})"
        parser.add_argument(flag, **kwargs)


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="med-risk-score",
        description="Clinical risk-score calculator",
    )
    sub = parser.add_subparsers(dest="command")

    # --- list ---
    sub.add_parser("list", help="List available risk scores")

    # --- info ---
    info_p = sub.add_parser("info", help="Show score details")
    info_p.add_argument("score_name", type=str, help="Score name")

    # --- compute ---
    compute_p = sub.add_parser("compute", help="Compute a risk score")
    compute_p.add_argument("score_name", type=str, help="Score name")
    compute_p.add_argument("--json", action="store_true", help="Read inputs as JSON from stdin")
    compute_p.add_argument("--pretty", action="store_true", help="Pretty-print JSON output")
    compute_p.add_argument("--all", action="store_true", help="Show all contributions")

    # Dynamic args added after score name is known — but we can do a two-pass approach
    # For simplicity, accept unknown args via parse_known_args
    return parser


def _format_result_text(result, *, show_all: bool = False) -> str:
    """Format a ScoreResult for human-readable terminal output."""
    lines = [
        f"Score:  {result.score_name}",
        f"Total:  {result.total_score}",
        f"Risk:   {result.risk_label}",
        f"Info:   {result.interpretation}",
    ]
    if show_all or result.contributions:
        lines.append("")
        lines.append("Contributions:")
        for k, v in result.contributions.items():
            lines.append(f"  {k:40s}  +{v:.1f}")
    if result.messages:
        lines.append("")
        for m in result.messages:
            lines.append(f"Note: {m}")
    return "\n".join(lines)


def main(argv: List[str] | None = None) -> int:
    parser = _build_parser()
    args, remaining = parser.parse_known_args(argv)

    if args.command is None:
        parser.print_help()
        return 0

    if args.command == "list":
        scores = list_scores()
        defs = all_definitions()
        print(f"{'Name':<25s} {'Display Name':<25s} Description")
        print("-" * 90)
        for name in scores:
            d = defs[name]
            print(f"{name:<25s} {d.display_name:<25s} {d.description[:50]}")
        return 0

    if args.command == "info":
        try:
            defn = get_score(args.score_name)
        except KeyError as exc:
            print(f"Error: {exc}", file=sys.stderr)
            return 1
        print(f"Score:     {defn.display_name} ({defn.name})")
        print(f"Version:   {defn.version}")
        print(f"Describe:  {defn.description}")
        print()
        print("Variables:")
        for v in defn.variables:
            parts = [f"  {v.name:30s}  {v.var_type:8s}  {v.description}"]
            if v.unit:
                parts[0] += f"  [{v.unit}]"
            if v.min_value is not None or v.max_value is not None:
                rng = f"[{v.min_value}..{v.max_value}]"
                parts[0] += f"  {rng}"
            if v.allowed_values:
                parts[0] += f"  allowed={list(v.allowed_values)}"
            print(parts[0])
        print()
        print("Risk categories:")
        for cat in defn.categories:
            print(f"  {cat.min_score:.0f}-{cat.max_score:.0f}  {cat.label:20s}  {cat.interpretation}")
        if defn.references:
            print()
            print("References:")
            for r in defn.references:
                print(f"  • {r}")
        return 0

    if args.command == "compute":
        score_name = args.score_name
        use_json = getattr(args, "json", False)
        pretty = getattr(args, "pretty", False)
        show_all = getattr(args, "all", False)

        if use_json:
            raw = sys.stdin.read()
            try:
                inputs = json.loads(raw)
            except json.JSONDecodeError as e:
                print(f"Error: invalid JSON input: {e}", file=sys.stderr)
                return 1
        else:
            # Collect remaining args: --key value pairs
            inputs = {}
            i = 0
            while i < len(remaining):
                arg = remaining[i]
                if arg.startswith("--"):
                    key = arg[2:].replace("-", "_")
                    if i + 1 < len(remaining) and not remaining[i + 1].startswith("--"):
                        val = remaining[i + 1]
                        # Try to parse as number, boolean, or string
                        if val.lower() in ("true", "yes"):
                            inputs[key] = True
                        elif val.lower() in ("false", "no"):
                            inputs[key] = False
                        else:
                            try:
                                inputs[key] = float(val)
                                if inputs[key] == int(inputs[key]):
                                    inputs[key] = int(inputs[key])
                            except ValueError:
                                inputs[key] = val
                        i += 2
                    else:
                        inputs[key] = True
                        i += 1
                else:
                    i += 1

        result_dict = compute_safe(score_name, inputs)
        if not result_dict["ok"]:
            errors = result_dict["errors"]
            print("Validation errors:", file=sys.stderr)
            for e in errors:
                print(f"  {e['variable']}: {e['message']}", file=sys.stderr)
            return 1

        if pretty or use_json:
            print(json.dumps(result_dict["result"], indent=2))
        else:
            # Reconstruct from dict
            from med_risk_scores.registry import ScoreResult, RiskCategory
            r = result_dict["result"]
            cat = RiskCategory(min_score=0, max_score=0, label=r["risk_label"], interpretation=r["interpretation"])
            sr = ScoreResult(
                score_name=r["score_name"],
                total_score=r["total_score"],
                category=cat,
                contributions=r["contributions"],
                raw_inputs=r["raw_inputs"],
                messages=r.get("messages", []),
            )
            print(_format_result_text(sr, show_all=show_all))
        return 0

    parser.print_help()
    return 0


if __name__ == "__main__":
    sys.exit(main())
