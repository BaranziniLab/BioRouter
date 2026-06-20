"""
Generic computation engine for clinical risk scores.

Orchestrates validation → computation → classification → result assembly.
"""
from __future__ import annotations

from typing import Any, Dict, List, Optional

from med_risk_scores.registry import ScoreDefinition, ScoreResult, get_score
from med_risk_scores.validate import validate_inputs, ValidationException


def compute(
    score_name: str,
    inputs: Dict[str, Any],
    *,
    strict: bool = True,
) -> ScoreResult:
    """
    Compute a clinical risk score.

    Parameters
    ----------
    score_name : str
        Name of the registered score (e.g. "cha2ds2_vasc").
    inputs : dict
        User-supplied variable values.
    strict : bool
        Whether to reject unknown input keys.

    Returns
    -------
    ScoreResult
        Contains total score, risk category, interpretation, and per-variable contributions.
    """
    defn = get_score(score_name)
    return compute_from_definition(defn, inputs, strict=strict)


def compute_from_definition(
    defn: ScoreDefinition,
    inputs: Dict[str, Any],
    *,
    strict: bool = True,
) -> ScoreResult:
    """Compute using an already-resolved ScoreDefinition."""
    # 1. Validate inputs
    validated = validate_inputs(defn.variables, inputs, strict=strict)

    # 2. Compute score + contributions
    total, contributions = defn.compute_fn(validated)

    # 3. Classify
    category = defn.classify(total)

    # 4. Build result
    messages: List[str] = []
    if total != sum(contributions.values()):
        messages.append(
            f"Note: total {total} != sum of contributions {sum(contributions.values()):.1f}"
        )

    return ScoreResult(
        score_name=defn.name,
        total_score=total,
        category=category,
        contributions=contributions,
        raw_inputs=inputs,
        messages=messages,
    )


def compute_safe(
    score_name: str,
    inputs: Dict[str, Any],
    *,
    strict: bool = True,
) -> Dict[str, Any]:
    """
    Compute a score and return a serialisable dict.
    Never raises – returns ``{"ok": False, "errors": [...]}`` on failure.
    """
    try:
        result = compute(score_name, inputs, strict=strict)
        return {"ok": True, "result": result.to_dict()}
    except ValidationException as exc:
        return {"ok": False, "errors": [{"variable": e.variable, "message": e.message} for e in exc.errors]}
    except Exception as exc:
        return {"ok": False, "errors": [{"variable": "*", "message": str(exc)}]}
