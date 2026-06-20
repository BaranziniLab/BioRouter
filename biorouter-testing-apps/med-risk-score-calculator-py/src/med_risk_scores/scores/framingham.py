"""
Framingham / ASCVD-style Cardiovascular Risk Scores.

Implements the Framingham Risk Score (FRS) for 10-year coronary heart disease risk
using the ATP-III / D'Agostino 2008 pooled-cohort equations as a simplified version.

Ref: D'Agostino RB Sr, et al. Circulation 2008.
     Wilson PWF, et al. Circulation 1998.
"""
from __future__ import annotations

import math
from typing import Any, Dict, List, Tuple

from med_risk_scores.registry import RiskCategory, ScoreResult, score_definition
from med_risk_scores.validate import VariableSpec

# ---------------------------------------------------------------------------
# Framingham Risk Score (simplified points-based, ATP-III)
# ---------------------------------------------------------------------------

FRS_VARIABLES: List[VariableSpec] = [
    VariableSpec(name="sex", description="Sex", var_type="enum", required=True, allowed_values=["male", "female"]),
    VariableSpec(name="age", description="Age in years", var_type="numeric", required=True, min_value=20, max_value=79, unit="years"),
    VariableSpec(name="total_cholesterol", description="Total cholesterol", var_type="numeric", required=True, min_value=100, max_value=400, unit="mg/dL"),
    VariableSpec(name="hdl_cholesterol", description="HDL cholesterol", var_type="numeric", required=True, min_value=20, max_value=150, unit="mg/dL"),
    VariableSpec(name="systolic_bp", description="Systolic blood pressure (untreated)", var_type="numeric", required=True, min_value=80, max_value=260, unit="mmHg"),
    VariableSpec(name="bp_treated", description="On antihypertensive medication", var_type="boolean", required=False, default=False),
    VariableSpec(name="smoker", description="Current smoker", var_type="boolean", required=True),
    VariableSpec(name="diabetes", description="Diabetes mellitus", var_type="boolean", required=False, default=False),
]

FRS_CATEGORIES: List[RiskCategory] = [
    RiskCategory(min_score=0, max_score=10, label="Low risk (< 10%)", interpretation="10-year CHD risk < 10%. Lifestyle modification; consider statin if additional risk factors.", color="#2ecc71"),
    RiskCategory(min_score=11, max_score=20, label="Moderate risk (10-20%)", interpretation="10-year CHD risk 10-20%. Lifestyle modification; consider aspirin and/or statin.", color="#f1c40f"),
    RiskCategory(min_score=21, max_score=100, label="High risk (> 20%)", interpretation="10-year CHD risk > 20%. Aggressive risk factor modification; aspirin + statin recommended.", color="#e74c3c"),
]

REFERENCES = [
    "D'Agostino RB Sr, et al. General cardiovascular risk profile for use in primary care. Circulation. 2008;117(6):743-53.",
    "Wilson PWF, et al. Prediction of coronary heart disease using risk factor categories. Circulation. 1998;97(18):1837-47.",
]


def _frs_points_male(age: float, tc: float, hdl: float, sbp: float, treated: bool, smoker: bool, diabetic: bool) -> float:
    pts = 0.0
    # Age
    if 20 <= age <= 34: pts += -9
    elif 35 <= age <= 39: pts += -4
    elif 40 <= age <= 44: pts += 0
    elif 45 <= age <= 49: pts += 3
    elif 50 <= age <= 54: pts += 6
    elif 55 <= age <= 59: pts += 8
    elif 60 <= age <= 64: pts += 10
    elif 65 <= age <= 69: pts += 11
    elif 70 <= age <= 74: pts += 12
    elif 75 <= age <= 79: pts += 13

    # Total cholesterol
    if tc < 160: pts += 0
    elif tc <= 199: pts += 0
    elif tc <= 239: pts += 1
    elif tc <= 279: pts += 2
    else: pts += 3

    # HDL
    if hdl >= 60: pts += -1
    elif hdl >= 50: pts += 0
    elif hdl >= 40: pts += 1
    else: pts += 2

    # SBP (untreated / treated)
    if sbp < 120: pts += 0
    elif sbp <= 129: pts += 0 if not treated else 1
    elif sbp <= 139: pts += 1 if not treated else 2
    elif sbp <= 159: pts += 1 if not treated else 2
    else: pts += 2 if not treated else 3

    # Smoking
    if smoker: pts += 2

    # Diabetes (men get 2 pts)
    if diabetic: pts += 2

    return pts


def _frs_points_female(age: float, tc: float, hdl: float, sbp: float, treated: bool, smoker: bool, diabetic: bool) -> float:
    pts = 0.0
    # Age
    if 20 <= age <= 34: pts += -7
    elif 35 <= age <= 39: pts += -3
    elif 40 <= age <= 44: pts += 0
    elif 45 <= age <= 49: pts += 3
    elif 50 <= age <= 54: pts += 6
    elif 55 <= age <= 59: pts += 8
    elif 60 <= age <= 64: pts += 10
    elif 65 <= age <= 69: pts += 12
    elif 70 <= age <= 74: pts += 14
    elif 75 <= age <= 79: pts += 16

    # Total cholesterol
    if tc < 160: pts += 0
    elif tc <= 199: pts += 1
    elif tc <= 239: pts += 1
    elif tc <= 279: pts += 2
    else: pts += 3

    # HDL
    if hdl >= 60: pts += -1
    elif hdl >= 50: pts += 0
    elif hdl >= 40: pts += 1
    else: pts += 2

    # SBP (untreated / treated)
    if sbp < 120: pts += 0
    elif sbp <= 129: pts += 1 if not treated else 3
    elif sbp <= 139: pts += 1 if not treated else 4
    elif sbp <= 159: pts += 2 if not treated else 5
    else: pts += 3 if not treated else 6

    # Smoking
    if smoker: pts += 3

    # Diabetes (women get 3 pts)
    if diabetic: pts += 3

    return pts


# Point threshold -> 10-year risk% mapping (ATP-III)
_RISK_MALE = {
    -2: 1, -1: 1, 0: 1, 1: 2, 2: 2, 3: 3, 4: 4, 5: 5,
    6: 7, 7: 8, 8: 10, 9: 11, 10: 14, 11: 16, 12: 19,
    13: 22, 14: 26, 15: 30, 16: 35, 17: 40, 18: 45, 19: 50, 20: 55,
}
_RISK_FEMALE = {
    -2: 1, -1: 1, 0: 1, 1: 1, 2: 2, 3: 2, 4: 3, 5: 4,
    6: 5, 7: 6, 8: 7, 9: 8, 10: 10, 11: 11, 12: 13,
    13: 15, 14: 17, 15: 20, 16: 24, 17: 27, 18: 31, 19: 35, 20: 40,
}


@score_definition(
    name="framingham_risk_score",
    display_name="Framingham Risk Score",
    description="10-year coronary heart disease risk (ATP-III points-based).",
    variables=FRS_VARIABLES,
    categories=FRS_CATEGORIES,
    references=REFERENCES,
)
def framingham_risk_score(inputs: Dict[str, Any]) -> Tuple[float, Dict[str, float]]:
    sex = inputs.get("sex", "male")
    age = inputs.get("age", 50)
    tc = inputs.get("total_cholesterol", 200)
    hdl = inputs.get("hdl_cholesterol", 50)
    sbp = inputs.get("systolic_bp", 120)
    treated = inputs.get("bp_treated", False)
    smoker = inputs.get("smoker", False)
    diabetes = inputs.get("diabetes", False)

    if sex == "male":
        pts = _frs_points_male(age, tc, hdl, sbp, treated, smoker, diabetes)
        risk_lookup = _RISK_MALE
    else:
        pts = _frs_points_female(age, tc, hdl, sbp, treated, smoker, diabetes)
        risk_lookup = _RISK_FEMALE

    # Map to risk percent
    clamped = max(min(pts, 20), -2)
    risk_pct = risk_lookup.get(int(clamped), 0)

    c: Dict[str, float] = {
        "FRS point total": float(int(pts)),
        "Estimated 10-year CHD risk (%)": float(risk_pct),
    }
    # Return points total as the "score" (category thresholds are on points)
    return float(int(pts)), c


# ---------------------------------------------------------------------------
# ASCVD Pooled Cohort Equation (simplified logistic-regression version)
# ---------------------------------------------------------------------------

ASCVD_VARIABLES: List[VariableSpec] = [
    VariableSpec(name="sex", description="Sex", var_type="enum", required=True, allowed_values=["male", "female"]),
    VariableSpec(name="race", description="Race", var_type="enum", required=True, allowed_values=["white", "african_american"]),
    VariableSpec(name="age", description="Age in years", var_type="numeric", required=True, min_value=40, max_value=79, unit="years"),
    VariableSpec(name="total_cholesterol", description="Total cholesterol", var_type="numeric", required=True, min_value=130, max_value=320, unit="mg/dL"),
    VariableSpec(name="hdl_cholesterol", description="HDL cholesterol", var_type="numeric", required=True, min_value=20, max_value=100, unit="mg/dL"),
    VariableSpec(name="systolic_bp", description="Systolic blood pressure", var_type="numeric", required=True, min_value=90, max_value=200, unit="mmHg"),
    VariableSpec(name="bp_treated", description="On antihypertensive medication", var_type="boolean", required=False, default=False),
    VariableSpec(name="smoker", description="Current smoker", var_type="boolean", required=True),
    VariableSpec(name="diabetes", description="Diabetes mellitus", var_type="boolean", required=False, default=False),
]

ASCVD_CATEGORIES: List[RiskCategory] = [
    RiskCategory(min_score=0, max_score=5, label="Low (< 5%)", interpretation="10-year ASCVD risk < 5%. Emphasise lifestyle.", color="#2ecc71"),
    RiskCategory(min_score=5, max_score=7.5, label="Borderline (5-7.5%)", interpretation="10-year ASCVD risk 5-7.5%. Consider risk-enhancers before statin.", color="#f1c40f"),
    RiskCategory(min_score=7.5, max_score=20, label="Intermediate (7.5-20%)", interpretation="10-year ASCVD risk 7.5-20%. Moderate-intensity statin recommended.", color="#e67e22"),
    RiskCategory(min_score=20, max_score=100, label="High (≥ 20%)", interpretation="10-year ASCVD risk ≥ 20%. High-intensity statin; consider aspirin.", color="#e74c3c"),
]


def _compute_ascvd_risk(
    sex: str, race: str, age: float, tc: float, hdl: float,
    sbp: float, treated: bool, smoker: bool, diabetes: bool,
) -> float:
    """
    Compute 10-year ASCVD risk % using 2013 ACC/AHA Pooled Cohort Equations.

    Uses mean-centered coefficients from the published Cox model.
    Reference: Goff DC Jr, et al. Circulation. 2014;129(25 Suppl 2):S49-73.
    """
    smoker_i = 1.0 if smoker else 0.0
    diabetes_i = 1.0 if diabetes else 0.0

    if sex == "male" and race == "white":
        s0 = 0.9144
        # White Male means: age=60.9, lnTC=5.18, lnHDL=3.96, lnSBP=4.89
        mean_age, mean_lnTC, mean_lnHDL, mean_lnSBP = 60.9, 5.18, 3.96, 4.89
        linear = (
            0.658 * (age - mean_age) / 10
            + 0.152 * (math.log(tc) - mean_lnTC)
            + (-0.263) * (math.log(hdl) - mean_lnHDL)
            + (0.181 if treated else 0.196) * (math.log(sbp) - mean_lnSBP)
            + 0.844 * smoker_i
            + 0.533 * diabetes_i
        )
    elif sex == "female" and race == "white":
        s0 = 0.9665
        mean_age, mean_lnTC, mean_lnHDL, mean_lnSBP = 60.9, 5.18, 3.96, 4.89
        linear = (
            0.876 * (age - mean_age) / 10
            + 0.195 * (math.log(tc) - mean_lnTC)
            + (-0.391) * (math.log(hdl) - mean_lnHDL)
            + (0.292 if treated else 0.107) * (math.log(sbp) - mean_lnSBP)
            + 0.591 * smoker_i
            + 0.290 * diabetes_i
        )
    elif sex == "male" and race == "african_american":
        s0 = 0.8954
        mean_age, mean_lnTC, mean_lnHDL, mean_lnSBP = 55.3, 5.18, 3.96, 4.89
        linear = (
            1.797 * (age - mean_age) / 10
            + 0.148 * (math.log(tc) - mean_lnTC)
            + (-0.141) * (math.log(hdl) - mean_lnHDL)
            + (0.645 if treated else 0.578) * (math.log(sbp) - mean_lnSBP)
            + 0.702 * smoker_i
            + 0.872 * diabetes_i
        )
    else:  # female, african_american
        s0 = 0.9533
        mean_age, mean_lnTC, mean_lnHDL, mean_lnSBP = 60.1, 5.18, 3.96, 4.89
        linear = (
            0.581 * (age - mean_age) / 10
            + 0.087 * (math.log(tc) - mean_lnTC)
            + (-0.538) * (math.log(hdl) - mean_lnHDL)
            + (1.016 if treated else 0.352) * (math.log(sbp) - mean_lnSBP)
            + 0.742 * smoker_i
            + 0.413 * diabetes_i
        )

    risk = 1.0 - s0 ** math.exp(linear)
    return max(0.0, min(round(risk * 100, 1), 100.0))


@score_definition(
    name="ascvd_10yr",
    display_name="ASCVD 10-Year Risk",
    description="Pooled Cohort Equations 10-year atherosclerotic cardiovascular disease risk.",
    variables=ASCVD_VARIABLES,
    categories=ASCVD_CATEGORIES,
    references=[
        "Goff DC Jr, et al. 2013 ACC/AHA guideline on the assessment of cardiovascular risk. Circulation. 2014;129(25 Suppl 2):S49-73.",
    ],
)
def ascvd_10yr(inputs: Dict[str, Any]) -> Tuple[float, Dict[str, float]]:
    risk_pct = _compute_ascvd_risk(
        sex=inputs.get("sex", "male"),
        race=inputs.get("race", "white"),
        age=inputs.get("age", 55),
        tc=inputs.get("total_cholesterol", 200),
        hdl=inputs.get("hdl_cholesterol", 50),
        sbp=inputs.get("systolic_bp", 130),
        treated=inputs.get("bp_treated", False),
        smoker=inputs.get("smoker", False),
        diabetes=inputs.get("diabetes", False),
    )
    c: Dict[str, float] = {
        "10-year ASCVD risk (%)": risk_pct,
    }
    return risk_pct, c
