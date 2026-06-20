"""Tests for HAS-BLED Bleeding Risk Score."""
import pytest
from med_risk_scores.engine import compute


class TestHasBled:
    """
    HAS-BLED:
      H – Uncontrolled hypertension: +1
      A – Abnormal renal:            +1
      A – Abnormal liver:            +1
      S – Stroke history:            +1
      B – Bleeding history:          +1
      L – Labile INR:                +1
      E – Elderly (> 65):            +1
      D – Drugs:                     +1
      D – Alcohol:                   +1
    Max = 9
    """

    def test_no_risk_factors(self):
        r = compute("has_bled", {
            "hypertension_uncontrolled": False,
            "renal_disease": False,
            "liver_disease": False,
            "stroke_history": False,
            "bleeding_history": False,
            "labile_inr": False,
            "elderly": False,
            "drugs": False,
            "alcohol": False,
        })
        assert r.total_score == 0
        assert r.risk_label == "Low"

    def test_hypertension_only(self):
        r = compute("has_bled", {
            "hypertension_uncontrolled": True,
            "renal_disease": False,
            "liver_disease": False,
            "stroke_history": False,
            "bleeding_history": False,
            "labile_inr": False,
            "elderly": False,
            "drugs": False,
            "alcohol": False,
        })
        assert r.total_score == 1
        assert r.risk_label == "Low"

    def test_renal_and_liver_each_plus_one(self):
        """Both renal and liver disease -> +2."""
        r = compute("has_bled", {
            "hypertension_uncontrolled": False,
            "renal_disease": True,
            "liver_disease": True,
            "stroke_history": False,
            "bleeding_history": False,
            "labile_inr": False,
            "elderly": False,
            "drugs": False,
            "alcohol": False,
        })
        assert r.total_score == 2
        assert r.risk_label == "Moderate"

    def test_drugs_and_alcohol_each_plus_one(self):
        r = compute("has_bled", {
            "hypertension_uncontrolled": False,
            "renal_disease": False,
            "liver_disease": False,
            "stroke_history": False,
            "bleeding_history": False,
            "labile_inr": False,
            "elderly": False,
            "drugs": True,
            "alcohol": True,
        })
        assert r.total_score == 2
        assert r.risk_label == "Moderate"

    def test_elderly_only(self):
        r = compute("has_bled", {
            "hypertension_uncontrolled": False,
            "renal_disease": False,
            "liver_disease": False,
            "stroke_history": False,
            "bleeding_history": False,
            "labile_inr": False,
            "elderly": True,
            "drugs": False,
            "alcohol": False,
        })
        assert r.total_score == 1
        assert r.risk_label == "Low"

    def test_all_risk_factors_max(self):
        r = compute("has_bled", {
            "hypertension_uncontrolled": True,
            "renal_disease": True,
            "liver_disease": True,
            "stroke_history": True,
            "bleeding_history": True,
            "labile_inr": True,
            "elderly": True,
            "drugs": True,
            "alcohol": True,
        })
        assert r.total_score == 9
        assert r.risk_label == "High"

    def test_score_3_high_risk(self):
        """Score >= 3 is high risk."""
        r = compute("has_bled", {
            "hypertension_uncontrolled": True,
            "renal_disease": True,
            "liver_disease": False,
            "stroke_history": True,
            "bleeding_history": False,
            "labile_inr": False,
            "elderly": False,
            "drugs": False,
            "alcohol": False,
        })
        assert r.total_score == 3
        assert r.risk_label == "High"

    def test_interpretation_mentions_anticoagulation(self):
        r = compute("has_bled", {
            "hypertension_uncontrolled": False,
            "renal_disease": False,
            "liver_disease": False,
            "stroke_history": False,
            "bleeding_history": False,
            "labile_inr": False,
            "elderly": False,
            "drugs": False,
            "alcohol": False,
        })
        assert "anticoagulation" in r.interpretation.lower()

    def test_missing_all_inputs_raises(self):
        with pytest.raises(Exception):
            compute("has_bled", {})
