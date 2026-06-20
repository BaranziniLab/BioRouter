"""
CURB-65 Severity Score for Community-Acquired Pneumonia.

Predicts 30-day mortality and guides disposition (outpatient vs inpatient).
Ref: Lim WS et al., Thorax 2003.
"""
from __future__ import annotations

from typing import Any, Dict, List, Tuple

from med_risk_scores.registry import RiskCategory, ScoreResult, score_definition
from med_risk_scores.validate import VariableSpec

VARIABLES: List[VariableSpec] = [
    VariableSpec(
        name="confusion",
        description="New-onset confusion (AMT ≤ 8 or disoriented)",
        var_type="boolean",
        required=True,
    ),
    VariableSpec(
        name="bun",
        description="Blood urea nitrogen (BUN)",
        var_type="numeric",
        required=True,
        min_value=0,
        max_value=200,
        unit="mg/dL",
    ),
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
        name="systolic_bp",
        description="Systolic blood pressure",
        var_type="numeric",
        required=True,
        min_value=50,
        max_value=300,
        unit="mmHg",
    ),
    VariableSpec(
        name="diastolic_bp",
        description="Diastolic blood pressure",
        var_type="numeric",
        required=True,
        min_value=20,
        max_value=200,
        unit="mmHg",
    ),
    VariableSpec(
        name="age",
        description="Age in years",
        var_type="numeric",
        required=True,
        min_value=0,
        max_value=130,
        unit="years",
    ),
]

CATEGORIES: List[RiskCategory] = [
    RiskCategory(min_score=0, max_score=0, label="Low risk (0)", interpretation="30-day mortality ~0.7%. Consider outpatient treatment.", color="#2ecc71"),
    RiskCategory(min_score=1, max_score=1, label="Low risk (1)", interpretation="30-day mortality ~3.2%. Consider outpatient with close follow-up.", color="#2ecc71"),
    RiskCategory(min_score=2, max_score=2, label="Moderate risk (2)", interpretation="30-day mortality ~13%. Hospital admission recommended.", color="#f1c40f"),
    RiskCategory(min_score=3, max_score=3, label="High risk (3)", interpretation="30-day mortality ~17%. Urgent hospital admission.", color="#e67e22"),
    RiskCategory(min_score=4, max_score=5, label="Very high risk (4-5)", interpretation="30-day mortality ~41%. Consider ICU admission.", color="#e74c3c"),
]

REFERENCES = [
    "Lim WS, et al. Defining community acquired pneumonia severity on presentation to hospital. Thorax. 2003;58(5):377-82.",
]


@score_definition(
    name="curb65",
    display_name="CURB-65",
    description="Severity score for community-acquired pneumonia (0–5 points).",
    variables=VARIABLES,
    categories=CATEGORIES,
    references=REFERENCES,
)
def curb65(inputs: Dict[str, Any]) -> Tuple[float, Dict[str, float]]:
    c = {}
    # C – Confusion
    c["Confusion"] = 1.0 if inputs.get("confusion", False) else 0.0
    # U – Urea (BUN ≥ 19 mg/dL)
    bun = inputs.get("bun", 0)
    c["BUN ≥ 19 mg/dL"] = 1.0 if bun >= 19 else 0.0
    # R – Respiratory rate ≥ 30
    rr = inputs.get("respiratory_rate", 0)
    c["RR ≥ 30"] = 1.0 if rr >= 30 else 0.0
    # B – Blood pressure (SBP < 90 or DBP ≤ 60)
    sbp = inputs.get("systolic_bp", 120)
    dbp = inputs.get("diastolic_bp", 80)
    c["BP < 90/60"] = 1.0 if sbp < 90 or dbp <= 60 else 0.0
    # 65 – Age ≥ 65
    age = inputs.get("age", 0)
    c["Age ≥ 65"] = 1.0 if age >= 65 else 0.0

    total = sum(c.values())
    return total, c
