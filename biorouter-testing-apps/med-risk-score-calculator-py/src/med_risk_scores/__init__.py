"""
med-risk-score-calculator
=========================

A composable clinical risk-score calculator library and CLI.

Implements validated clinical risk scores as declarative models with:
- Input variable specs (types, units, valid ranges)
- Point/contribution computation rules
- Risk category interpretation with recommendations
- Full input validation with structured error messages
- Unit conversion helpers

Quick start::

    from med_risk_scores import compute
    result = compute("cha2ds2_vasc", {
        "chf": False, "hypertension": True, "age": 72,
        "diabetes": True, "stroke_tia": False,
        "vascular_disease": False, "sex_female": True,
    })
    print(result.total_score, result.risk_label)
"""
from med_risk_scores.engine import compute, compute_from_definition, compute_safe
from med_risk_scores.registry import get_score, list_scores, all_definitions, ScoreResult
from med_risk_scores.validate import ValidationException, ValidationError
from med_risk_scores import units

# Force registration of all built-in scores
from med_risk_scores import scores  # noqa: F401

__version__ = "1.0.0"
__all__ = [
    "compute",
    "compute_from_definition",
    "compute_safe",
    "get_score",
    "list_scores",
    "all_definitions",
    "ScoreResult",
    "ValidationException",
    "ValidationError",
    "units",
]
