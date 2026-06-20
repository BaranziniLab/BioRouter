"""
MELD (Model for End-Stage Liver Disease) Score.

Predicts 3-month mortality in liver disease; used for transplant prioritisation.
Implements both classic MELD and MELD-Na.
Ref: Malinchoc M et al., Hepatology 2000; Leise MD et al., Liver Transpl 2014.
"""
from __future__ import annotations

import math
from typing import Any, Dict, List, Tuple

from med_risk_scores.registry import RiskCategory, ScoreResult, score_definition
from med_risk_scores.validate import VariableSpec

# ---------------------------------------------------------------------------
# MELD (classic)
# ---------------------------------------------------------------------------

MELD_VARIABLES: List[VariableSpec] = [
    VariableSpec(
        name="bilirubin",
        description="Total serum bilirubin",
        var_type="numeric",
        required=True,
        min_value=0.1,
        max_value=100,
        unit="mg/dL",
    ),
    VariableSpec(
        name="inr",
        description="International normalised ratio",
        var_type="numeric",
        required=True,
        min_value=0.5,
        max_value=10,
    ),
    VariableSpec(
        name="creatinine",
        description="Serum creatinine",
        var_type="numeric",
        required=True,
        min_value=0.1,
        max_value=30,
        unit="mg/dL",
    ),
    VariableSpec(
        name="dialysis",
        description="On dialysis (overrides creatinine)",
        var_type="boolean",
        required=False,
        default=False,
    ),
]

MELD_CATEGORIES: List[RiskCategory] = [
    RiskCategory(min_score=0, max_score=9, label="Low severity", interpretation="MELD < 10: minimal liver disease severity.", color="#2ecc71"),
    RiskCategory(min_score=10, max_score=19, label="Moderate severity", interpretation="MELD 10-19: progressive liver dysfunction.", color="#f1c40f"),
    RiskCategory(min_score=20, max_score=29, label="High severity", interpretation="MELD 20-29: significant mortality risk; transplant evaluation warranted.", color="#e67e22"),
    RiskCategory(min_score=30, max_score=40, label="Critical severity", interpretation="MELD ≥ 30: very high mortality; high transplant priority.", color="#e74c3c"),
]


def _meld_core(inputs: Dict[str, Any], use_na: bool = False) -> Tuple[float, Dict[str, float]]:
    """Core MELD calculation shared by MELD and MELD-Na."""
    bili = max(inputs.get("bilirubin", 1.0), 1.0)
    inr_val = max(inputs.get("inr", 1.0), 1.0)
    cr = max(inputs.get("creatinine", 1.0), 1.0)
    dialysis = inputs.get("dialysis", False)

    # Creatinine floor at 4.0 if on dialysis
    if dialysis:
        cr = max(cr, 4.0)

    meld = 3.78 * math.log(bili) + 11.2 * math.log(inr_val) + 9.57 * math.log(cr) + 6.43

    c: Dict[str, float] = {
        f"3.78 × ln(bilirubin={bili:.1f})": 3.78 * math.log(bili),
        f"11.2 × ln(INR={inr_val:.1f})": 11.2 * math.log(inr_val),
        f"9.57 × ln(creatinine={cr:.1f})": 9.57 * math.log(cr),
        "Constant (6.43)": 6.43,
    }

    if use_na:
        na = inputs.get("sodium", 140.0)
        na = max(min(na, 145.0), 125.0)
        meld_na_correction = 1.32 * (137 - na) - (0.033 * meld * (137 - na))
        meld += meld_na_correction
        c[f"Na correction ({na:.0f} mmol/L)"] = meld_na_correction

    # Floor at 6, ceiling at 40
    meld = max(min(round(meld), 40), 6)
    return meld, c


@score_definition(
    name="meld",
    display_name="MELD",
    description="Model for End-Stage Liver Disease score (classic).",
    variables=MELD_VARIABLES,
    categories=MELD_CATEGORIES,
    references=["Malinchoc M, et al. Hepatology. 2000;31(4):864-70."],
)
def meld(inputs: Dict[str, Any]) -> Tuple[float, Dict[str, float]]:
    return _meld_core(inputs, use_na=False)


# ---------------------------------------------------------------------------
# MELD-Na
# ---------------------------------------------------------------------------

MELDNA_VARIABLES: List[VariableSpec] = [
    VariableSpec(name="bilirubin", description="Total serum bilirubin", var_type="numeric", required=True, min_value=0.1, max_value=100, unit="mg/dL"),
    VariableSpec(name="inr", description="International normalised ratio", var_type="numeric", required=True, min_value=0.5, max_value=10),
    VariableSpec(name="creatinine", description="Serum creatinine", var_type="numeric", required=True, min_value=0.1, max_value=30, unit="mg/dL"),
    VariableSpec(name="dialysis", description="On dialysis", var_type="boolean", required=False, default=False),
    VariableSpec(name="sodium", description="Serum sodium", var_type="numeric", required=True, min_value=125, max_value=145, unit="mmol/L"),
]


@score_definition(
    name="meld_na",
    display_name="MELD-Na",
    description="MELD incorporating serum sodium for improved mortality prediction.",
    variables=MELDNA_VARIABLES,
    categories=MELD_CATEGORIES,
    references=[
        "Malinchoc M, et al. Hepatology. 2000;31(4):864-70.",
        "Leise MD, et al. Liver Transpl. 2014;20(5):S25.",
    ],
)
def meld_na(inputs: Dict[str, Any]) -> Tuple[float, Dict[str, float]]:
    return _meld_core(inputs, use_na=True)
