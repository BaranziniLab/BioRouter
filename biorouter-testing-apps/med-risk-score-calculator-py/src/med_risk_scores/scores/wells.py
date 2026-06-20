"""
Wells Score for DVT and Pulmonary Embolism.

Two variants:
  - wells_dvt: Wells criteria for DVT (modified by Wells et al. 2003)
  - wells_pe: Wells criteria for PE (Wells et al. 2001, refined by Wicki et al.)

Ref: Wells PS et al., Ann Intern Med 2001, Thromb Haemost 2003.
"""
from __future__ import annotations

from typing import Any, Dict, List, Tuple

from med_risk_scores.registry import RiskCategory, ScoreResult, score_definition
from med_risk_scores.validate import VariableSpec

# ---------------------------------------------------------------------------
# DVT variant
# ---------------------------------------------------------------------------

DVT_VARIABLES: List[VariableSpec] = [
    VariableSpec(name="active_cancer", description="Active cancer (treatment within 6 mo or palliative)", var_type="boolean", required=True),
    VariableSpec(name="paralysis", description="Paralysis, paresis, or recent plaster immobilisation of lower extremity", var_type="boolean", required=True),
    VariableSpec(name="bedridden", description="Recently bedridden > 3 days or major surgery within 12 weeks", var_type="boolean", required=True),
    VariableSpec(name="localized_tenderness", description="Localized tenderness along the deep venous system", var_type="boolean", required=True),
    VariableSpec(name="entire_leg_swollen", description="Entire leg swollen", var_type="boolean", required=True),
    VariableSpec(name="calf_swelling", description="Calf swelling ≥ 3 cm compared to asymptomatic side", var_type="boolean", required=True),
    VariableSpec(name="pitting_edema", description="Pitting edema (greater in symptomatic leg)", var_type="boolean", required=True),
    VariableSpec(name="collateral_veins", description="Collateral superficial veins (non-varicose)", var_type="boolean", required=True),
    VariableSpec(name="alternative_diagnosis", description="Alternative diagnosis as likely or greater than DVT", var_type="boolean", required=True),
]

DVT_CATEGORIES: List[RiskCategory] = [
    RiskCategory(min_score=-2, max_score=1, label="Low probability", interpretation="DVT unlikely; consider D-dimer to rule out.", color="#2ecc71"),
    RiskCategory(min_score=2, max_score=3, label="Moderate probability", interpretation="DVT moderately likely; duplex ultrasound recommended.", color="#f1c40f"),
    RiskCategory(min_score=4, max_score=8, label="High probability", interpretation="DVT highly likely; duplex ultrasound indicated.", color="#e74c3c"),
]


@score_definition(
    name="wells_dvt",
    display_name="Wells Score (DVT)",
    description="Wells clinical prediction rule for deep vein thrombosis.",
    variables=DVT_VARIABLES,
    categories=DVT_CATEGORIES,
    references=["Wells PS, et al. Ann Intern Med. 2003;139(2):104-113."],
)
def wells_dvt(inputs: Dict[str, Any]) -> Tuple[float, Dict[str, float]]:
    c = {}
    c["Active cancer"] = 1.0 if inputs.get("active_cancer") else 0.0
    c["Paralysis / immobilisation"] = 1.0 if inputs.get("paralysis") else 0.0
    c["Bedridden / recent surgery"] = 1.0 if inputs.get("bedridden") else 0.0
    c["Localized tenderness"] = 1.0 if inputs.get("localized_tenderness") else 0.0
    c["Entire leg swollen"] = 1.0 if inputs.get("entire_leg_swollen") else 0.0
    c["Calf swelling ≥ 3 cm"] = 1.0 if inputs.get("calf_swelling") else 0.0
    c["Pitting edema"] = 1.0 if inputs.get("pitting_edema") else 0.0
    c["Collateral veins"] = 1.0 if inputs.get("collateral_veins") else 0.0
    c["Alternative diagnosis"] = -2.0 if inputs.get("alternative_diagnosis") else 0.0
    return sum(c.values()), c


# ---------------------------------------------------------------------------
# PE variant
# ---------------------------------------------------------------------------

PE_VARIABLES: List[VariableSpec] = [
    VariableSpec(name="dvt_symptoms", description="Clinical signs/symptoms of DVT", var_type="boolean", required=True),
    VariableSpec(name="pe_number1", description="PE is #1 diagnosis or equally likely", var_type="boolean", required=True),
    VariableSpec(name="heart_rate", description="Heart rate > 100 bpm", var_type="numeric", required=True, min_value=30, max_value=300, unit="bpm"),
    VariableSpec(name="immobilization", description="Immobolisation ≥ 3 days or surgery within 4 weeks", var_type="boolean", required=True),
    VariableSpec(name="prior_pe_dvt", description="Previous PE or DVT", var_type="boolean", required=True),
    VariableSpec(name="hemoptysis", description="Hemoptysis", var_type="boolean", required=True),
    VariableSpec(name="malignancy", description="Malignancy (treatment within 6 months or palliative)", var_type="boolean", required=True),
]

PE_CATEGORIES: List[RiskCategory] = [
    RiskCategory(min_score=0, max_score=1, label="Low probability", interpretation="PE unlikely; D-dimer may help rule out.", color="#2ecc71"),
    RiskCategory(min_score=2, max_score=3, label="Moderate probability", interpretation="PE possible; CT pulmonary angiography recommended.", color="#f1c40f"),
    RiskCategory(min_score=4, max_score=12, label="High probability", interpretation="PE likely; proceed to imaging.", color="#e74c3c"),
]


@score_definition(
    name="wells_pe",
    display_name="Wells Score (PE)",
    description="Wells clinical prediction rule for pulmonary embolism.",
    variables=PE_VARIABLES,
    categories=PE_CATEGORIES,
    references=["Wells PS, et al. Thromb Haemost. 2001;85(1):18-22."],
)
def wells_pe(inputs: Dict[str, Any]) -> Tuple[float, Dict[str, float]]:
    c = {}
    c["DVT symptoms"] = 3.0 if inputs.get("dvt_symptoms") else 0.0
    c["PE #1 diagnosis"] = 3.0 if inputs.get("pe_number1") else 0.0
    c["HR > 100"] = 1.5 if inputs.get("heart_rate", 0) > 100 else 0.0
    c["Immobilisation / surgery"] = 1.5 if inputs.get("immobilization") else 0.0
    c["Prior PE/DVT"] = 1.5 if inputs.get("prior_pe_dvt") else 0.0
    c["Hemoptysis"] = 1.0 if inputs.get("hemoptysis") else 0.0
    c["Malignancy"] = 1.0 if inputs.get("malignancy") else 0.0
    return sum(c.values()), c
