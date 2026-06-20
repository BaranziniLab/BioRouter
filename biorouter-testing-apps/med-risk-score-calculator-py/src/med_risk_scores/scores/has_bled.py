"""
HAS-BLED Bleeding Risk Score.

Estimates 1-year major bleeding risk in atrial fibrillation patients.
Ref: Pisters R et al., Chest 2010.
"""
from __future__ import annotations

from typing import Any, Dict, List, Tuple

from med_risk_scores.registry import RiskCategory, ScoreResult, score_definition
from med_risk_scores.validate import VariableSpec

VARIABLES: List[VariableSpec] = [
    VariableSpec(
        name="hypertension_uncontrolled",
        description="Uncontrolled hypertension (systolic > 160 mmHg)",
        var_type="boolean",
        required=True,
    ),
    VariableSpec(
        name="renal_disease",
        description="Abnormal renal function (dialysis, transplant, Cr > 200 µmol/L)",
        var_type="boolean",
        required=True,
    ),
    VariableSpec(
        name="liver_disease",
        description="Abnormal liver function (cirrhosis, bilirubin > 2× ULN, AST/ALT > 3× ULN)",
        var_type="boolean",
        required=True,
    ),
    VariableSpec(
        name="stroke_history",
        description="Prior stroke history",
        var_type="boolean",
        required=True,
    ),
    VariableSpec(
        name="bleeding_history",
        description="Prior major bleeding or predisposition",
        var_type="boolean",
        required=True,
    ),
    VariableSpec(
        name="labile_inr",
        description="Labile INR (TTR < 60%)",
        var_type="boolean",
        required=True,
    ),
    VariableSpec(
        name="elderly",
        description="Age > 65 years",
        var_type="boolean",
        required=True,
    ),
    VariableSpec(
        name="drugs",
        description="Concomitant antiplatelet agents or NSAIDs",
        var_type="boolean",
        required=True,
    ),
    VariableSpec(
        name="alcohol",
        description="Excessive alcohol intake (> 8 drinks/week)",
        var_type="boolean",
        required=True,
    ),
]

CATEGORIES: List[RiskCategory] = [
    RiskCategory(min_score=0, max_score=0, label="Low", interpretation="Annual bleeding risk ~1.0%; anticoagulation generally safe.", color="#2ecc71"),
    RiskCategory(min_score=1, max_score=1, label="Low", interpretation="Annual bleeding risk ~1.0%; anticoagulation generally safe.", color="#2ecc71"),
    RiskCategory(min_score=2, max_score=2, label="Moderate", interpretation="Annual bleeding risk ~1.9%; careful monitoring recommended.", color="#f1c40f"),
    RiskCategory(min_score=3, max_score=9, label="High", interpretation="Annual bleeding risk ≥ 3.7%; consider limiting therapy duration and simplifying regimens. NOT a contraindication.", color="#e74c3c"),
]

REFERENCES = [
    "Pisters R, et al. A novel user-friendly score (HAS-BLED) to assess 1-year risk of major bleeding in AF patients. Chest. 2010;138(5):1093-100.",
]


@score_definition(
    name="has_bled",
    display_name="HAS-BLED",
    description="Major bleeding risk score for atrial fibrillation patients (0–9 points).",
    variables=VARIABLES,
    categories=CATEGORIES,
    references=REFERENCES,
)
def has_bled(inputs: Dict[str, Any]) -> Tuple[float, Dict[str, float]]:
    c = {}

    c["H – Hypertension (uncontrolled)"] = 1.0 if inputs.get("hypertension_uncontrolled", False) else 0.0
    c["A – Abnormal renal/liver function"] = (1.0 if inputs.get("renal_disease", False) else 0.0) + \
                                              (1.0 if inputs.get("liver_disease", False) else 0.0)
    c["S – Stroke history"] = 1.0 if inputs.get("stroke_history", False) else 0.0
    c["B – Bleeding history"] = 1.0 if inputs.get("bleeding_history", False) else 0.0
    c["L – Labile INR"] = 1.0 if inputs.get("labile_inr", False) else 0.0
    c["E – Elderly (> 65)"] = 1.0 if inputs.get("elderly", False) else 0.0
    c["D – Drugs/alcohol"] = (1.0 if inputs.get("drugs", False) else 0.0) + \
                              (1.0 if inputs.get("alcohol", False) else 0.0)

    total = sum(c.values())
    return total, c
