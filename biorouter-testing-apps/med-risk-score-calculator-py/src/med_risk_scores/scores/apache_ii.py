"""
APACHE II-lite (Simplified Acute Physiology Score).

A simplified version of the APACHE II scoring system that uses a subset
of the 12 acute physiology variables for rapid bedside estimation.

Full APACHE II: Knaus WA et al., Crit Care Med 1985.
This "lite" version covers the most discriminating physiology items.
"""
from __future__ import annotations

from typing import Any, Dict, List, Tuple

from med_risk_scores.registry import RiskCategory, ScoreResult, score_definition
from med_risk_scores.validate import VariableSpec

VARIABLES: List[VariableSpec] = [
    VariableSpec(name="temperature", description="Rectal temperature", var_type="numeric", required=True, min_value=28, max_value=42, unit="C"),
    VariableSpec(name="mean_arterial_pressure", description="Mean arterial pressure (MAP)", var_type="numeric", required=True, min_value=30, max_value=250, unit="mmHg"),
    VariableSpec(name="heart_rate", description="Heart rate", var_type="numeric", required=True, min_value=30, max_value=250, unit="bpm"),
    VariableSpec(name="respiratory_rate", description="Respiratory rate", var_type="numeric", required=True, min_value=0, max_value=80, unit="/min"),
    VariableSpec(name="oxygenation", description="PaO₂/FiO₂ ratio or A-a gradient. Provide PaO₂ on room air (mmHg).", var_type="numeric", required=True, min_value=20, max_value=600, unit="mmHg"),
    VariableSpec(name="arterial_pH", description="Arterial pH", var_type="numeric", required=True, min_value=6.8, max_value=7.8),
    VariableSpec(name="sodium", description="Serum sodium", var_type="numeric", required=True, min_value=110, max_value=180, unit="mmol/L"),
    VariableSpec(name="potassium", description="Serum potassium", var_type="numeric", required=True, min_value=1.5, max_value=8, unit="mmol/L"),
    VariableSpec(name="creatinine", description="Serum creatinine", var_type="numeric", required=True, min_value=0.1, max_value=30, unit="mg/dL"),
    VariableSpec(name="hematocrit", description="Hematocrit (%)", var_type="numeric", required=True, min_value=10, max_value=65, unit="%"),
    VariableSpec(name="wbc", description="White blood cell count", var_type="numeric", required=True, min_value=0, max_value=100, unit="×10³/µL"),
    VariableSpec(name="gcs", description="Glasgow Coma Score (3-15)", var_type="numeric", required=True, min_value=3, max_value=15),
    VariableSpec(name="age", description="Age in years", var_type="numeric", required=True, min_value=0, max_value=120, unit="years"),
    VariableSpec(name="chronic_health", description="Severe organ insufficiency or immunocompromised", var_type="boolean", required=False, default=False),
]


def _aps_temperature(t: float) -> int:
    if t <= 29.9: return 4
    elif t <= 31.9: return 3
    elif t <= 33.9: return 2
    elif t <= 35.9: return 1
    elif t <= 38.4: return 0
    elif t <= 38.9: return 1
    elif t <= 39.9: return 3
    else: return 4


def _aps_map(m: float) -> int:
    if m <= 49: return 4
    elif m <= 69: return 2
    elif m <= 149: return 0
    elif m <= 169: return 2
    else: return 4


def _aps_hr(h: float) -> int:
    if h <= 39: return 4
    elif h <= 59: return 2
    elif h <= 139: return 0
    elif h <= 159: return 2
    else: return 4


def _aps_rr(rr: float) -> int:
    if rr <= 5: return 4
    elif rr <= 11: return 1
    elif rr <= 24: return 0
    elif rr <= 34: return 1
    elif rr <= 39: return 3
    else: return 4


def _aps_oxygen(pao2: float) -> int:
    """Simplified: use PaO₂ on room air."""
    if pao2 < 55: return 4
    elif pao2 < 60: return 3
    elif pao2 < 70: return 2
    elif pao2 < 75: return 1
    else: return 0


def _aps_ph(ph: float) -> int:
    if ph < 7.15: return 4
    elif ph < 7.25: return 3
    elif ph < 7.32: return 2
    elif ph < 7.35: return 1
    elif ph <= 7.45: return 0
    elif ph <= 7.50: return 1
    elif ph <= 7.60: return 3
    else: return 4


def _aps_na(na: float) -> int:
    if na < 120: return 4
    elif na < 130: return 2
    elif na <= 149: return 0
    elif na <= 159: return 2
    else: return 4


def _aps_k(k: float) -> int:
    if k < 3.0: return 4
    elif k < 3.5: return 2
    elif k <= 5.0: return 0
    elif k <= 5.9: return 2
    else: return 4


def _aps_cr(cr: float) -> int:
    if cr < 0.6: return 2
    elif cr <= 1.4: return 0
    elif cr <= 1.9: return 2
    elif cr <= 3.4: return 3
    else: return 4


def _aps_hct(hct: float) -> int:
    if hct < 20: return 4
    elif hct < 30: return 2
    elif hct < 46: return 0
    elif hct <= 50: return 2
    else: return 4


def _aps_wbc(wbc: float) -> int:
    if wbc < 1.0: return 4
    elif wbc < 3.0: return 2
    elif wbc <= 14.9: return 0
    elif wbc <= 24.9: return 2
    else: return 4


def _age_points(age: float) -> int:
    if age < 45: return 0
    elif age <= 54: return 2
    elif age <= 64: return 3
    elif age <= 74: return 5
    else: return 6


CATEGORIES: List[RiskCategory] = [
    RiskCategory(min_score=0, max_score=4, label="Mild illness", interpretation="Predicted mortality < 4%. ICU monitoring but lower acuity.", color="#2ecc71"),
    RiskCategory(min_score=5, max_score=9, label="Moderate illness", interpretation="Predicted mortality 4-8%. Active ICU management.", color="#f1c40f"),
    RiskCategory(min_score=10, max_score=14, label="Moderate-severe", interpretation="Predicted mortality 15-20%. Aggressive support.", color="#e67e22"),
    RiskCategory(min_score=15, max_score=19, label="Severe illness", interpretation="Predicted mortality 20-40%. Intensive monitoring.", color="#c0392b"),
    RiskCategory(min_score=20, max_score=71, label="Very severe illness", interpretation="Predicted mortality > 40%. Maximum life-support measures.", color="#e74c3c"),
]

REFERENCES = [
    "Knaus WA, et al. APACHE II: a severity of disease classification system. Crit Care Med. 1985;13(10):818-29.",
]


@score_definition(
    name="apache_ii_lite",
    display_name="APACHE II-lite",
    description="Simplified Acute Physiology Score for ICU severity (0–71 points).",
    variables=VARIABLES,
    categories=CATEGORIES,
    references=REFERENCES,
)
def apache_ii_lite(inputs: Dict[str, Any]) -> Tuple[float, Dict[str, float]]:
    c: Dict[str, float] = {}

    c["Temperature"] = float(_aps_temperature(inputs.get("temperature", 37.0)))
    c["MAP"] = float(_aps_map(inputs.get("mean_arterial_pressure", 80)))
    c["Heart rate"] = float(_aps_hr(inputs.get("heart_rate", 80)))
    c["Respiratory rate"] = float(_aps_rr(inputs.get("respiratory_rate", 16)))
    c["Oxygenation (PaO₂)"] = float(_aps_oxygen(inputs.get("oxygenation", 90)))
    c["Arterial pH"] = float(_aps_ph(inputs.get("arterial_pH", 7.40)))
    c["Sodium"] = float(_aps_na(inputs.get("sodium", 140)))
    c["Potassium"] = float(_aps_k(inputs.get("potassium", 4.0)))
    c["Creatinine"] = float(_aps_cr(inputs.get("creatinine", 1.0)))
    c["Hematocrit"] = float(_aps_hct(inputs.get("hematocrit", 40)))
    c["WBC"] = float(_aps_wbc(inputs.get("wbc", 10)))
    c["GCS points (15 - GCS)"] = float(15 - inputs.get("gcs", 15))

    phys_score = sum(c.values())

    # Age points
    age_pts = _age_points(inputs.get("age", 50))
    c["Age points"] = float(age_pts)

    # Chronic health points
    chronic_pts = 5.0 if inputs.get("chronic_health", False) else 0.0
    c["Chronic health points"] = chronic_pts

    total = phys_score + age_pts + chronic_pts
    return total, c
