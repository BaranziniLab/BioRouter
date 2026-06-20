"""
CHA₂DS₂-VASc Stroke Risk Score.

Assesses stroke risk in patients with non-valvular atrial fibrillation.
Ref: Lip GY et al., Chest 2010.
"""
from __future__ import annotations

from typing import Any, Dict, List, Tuple

from med_risk_scores.registry import RiskCategory, ScoreResult, score_definition
from med_risk_scores.validate import VariableSpec

VARIABLES: List[VariableSpec] = [
    VariableSpec(
        name="chf",
        description="Congestive Heart Failure (or LV dysfunction)",
        var_type="boolean",
        required=True,
    ),
    VariableSpec(
        name="hypertension",
        description="Hypertension",
        var_type="boolean",
        required=True,
    ),
    VariableSpec(
        name="age",
        description="Patient age in years",
        var_type="numeric",
        required=True,
        min_value=0,
        max_value=130,
        unit="years",
    ),
    VariableSpec(
        name="diabetes",
        description="Diabetes mellitus",
        var_type="boolean",
        required=True,
    ),
    VariableSpec(
        name="stroke_tia",
        description="Prior stroke, TIA, or thromboembolism",
        var_type="boolean",
        required=True,
    ),
    VariableSpec(
        name="vascular_disease",
        description="Vascular disease (prior MI, PAD, aortic plaque)",
        var_type="boolean",
        required=True,
    ),
    VariableSpec(
        name="sex_female",
        description="Sex category – female",
        var_type="boolean",
        required=True,
    ),
]

CATEGORIES: List[RiskCategory] = [
    RiskCategory(min_score=0, max_score=0, label="Low", interpretation="Low stroke risk; consider no anticoagulation.", color="#2ecc71"),
    RiskCategory(min_score=1, max_score=1, label="Low-Moderate", interpretation="Low-moderate stroke risk; anticoagulation should be considered.", color="#f1c40f"),
    RiskCategory(min_score=2, max_score=3, label="Moderate", interpretation="Moderate stroke risk; anticoagulation recommended.", color="#e67e22"),
    RiskCategory(min_score=4, max_score=9, label="High", interpretation="High stroke risk; anticoagulation strongly recommended.", color="#e74c3c"),
]

REFERENCES = [
    "Lip GY, et al. Refining clinical risk stratification: a new CHA2DS2-VASc score. Chest. 2010;137(2):263-72.",
    "Lanctôt KL, et al. CHA2DS2-VASc score for stroke risk. Ann Pharmacother. 2014.",
]


@score_definition(
    name="cha2ds2_vasc",
    display_name="CHA₂DS₂-VASc",
    description="Stroke risk score for non-valvular atrial fibrillation (0–9 points).",
    variables=VARIABLES,
    categories=CATEGORIES,
    references=REFERENCES,
)
def cha2ds2_vasc(inputs: Dict[str, Any]) -> Tuple[float, Dict[str, float]]:
    c = {}

    # C – CHF / LV dysfunction
    c["CHF/LV dysfunction"] = 1.0 if inputs.get("chf", False) else 0.0

    # H – Hypertension
    c["Hypertension"] = 1.0 if inputs.get("hypertension", False) else 0.0

    # A2 – Age >= 75
    age = inputs.get("age", 0)
    c["Age ≥ 75"] = 2.0 if age >= 75 else 0.0

    # D – Diabetes
    c["Diabetes"] = 1.0 if inputs.get("diabetes", False) else 0.0

    # S2 – Stroke / TIA / thromboembolism
    c["Prior stroke/TIA/TE"] = 2.0 if inputs.get("stroke_tia", False) else 0.0

    # V – Vascular disease
    c["Vascular disease"] = 1.0 if inputs.get("vascular_disease", False) else 0.0

    # A – Age 65–74
    c["Age 65-74"] = 1.0 if 65 <= age < 75 else 0.0

    # Sc – Sex category (female)
    c["Female sex"] = 1.0 if inputs.get("sex_female", False) else 0.0

    total = sum(c.values())
    return total, c
