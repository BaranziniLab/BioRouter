"""
qSOFA (Quick Sequential Organ Failure Assessment) for Sepsis Screening.

Bedside screening tool to identify patients with suspected infection who are
at risk of poor outcomes (≥ 2 suggests sepsis with organ dysfunction).
Ref: Singer M et al., JAMA 2016.
"""
from __future__ import annotations

from typing import Any, Dict, List, Tuple

from med_risk_scores.registry import RiskCategory, ScoreResult, score_definition
from med_risk_scores.validate import VariableSpec

VARIABLES: List[VariableSpec] = [
    VariableSpec(
        name="respiratory_rate",
        description="Respiratory rate",
        var_type="numeric",
        required=True,
        min_value=5,
        max_value=80,
        unit="/min",
    ),
    VariableSpec(
        name="altered_mentation",
        description="Altered mentation (GCS < 15)",
        var_type="boolean",
        required=True,
    ),
    VariableSpec(
        name="systolic_bp",
        description="Systolic blood pressure",
        var_type="numeric",
        required=True,
        min_value=50,
        max_value=300,
        unit="mmHg",
    ),
]

CATEGORIES: List[RiskCategory] = [
    RiskCategory(min_score=0, max_score=0, label="Low risk", interpretation="qSOFA < 2: sepsis unlikely. Standard care.", color="#2ecc71"),
    RiskCategory(min_score=1, max_score=1, label="Low risk", interpretation="qSOFA < 2: sepsis unlikely. Standard care.", color="#2ecc71"),
    RiskCategory(min_score=2, max_score=3, label="High risk", interpretation="qSOFA ≥ 2: high risk of poor outcome in suspected infection. Consider sepsis workup and organ support.", color="#e74c3c"),
]

REFERENCES = [
    "Singer M, et al. The Third International Consensus Definitions for Sepsis and Septic Shock (Sepsis-3). JAMA. 2016;315(8):801-10.",
]


@score_definition(
    name="qsofa",
    display_name="qSOFA",
    description="Quick SOFA for bedside sepsis screening (0–3 points).",
    variables=VARIABLES,
    categories=CATEGORIES,
    references=REFERENCES,
)
def qsofa(inputs: Dict[str, Any]) -> Tuple[float, Dict[str, float]]:
    c = {}
    # RR ≥ 22
    c["Respiratory rate ≥ 22"] = 1.0 if inputs.get("respiratory_rate", 0) >= 22 else 0.0
    # Altered mentation
    c["Altered mentation"] = 1.0 if inputs.get("altered_mentation", False) else 0.0
    # SBP ≤ 100
    c["Systolic BP ≤ 100"] = 1.0 if inputs.get("systolic_bp", 120) <= 100 else 0.0

    total = sum(c.values())
    return total, c
