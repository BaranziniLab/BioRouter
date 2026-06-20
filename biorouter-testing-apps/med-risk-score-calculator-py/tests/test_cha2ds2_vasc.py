"""Tests for CHA₂DS₂-VASc Stroke Risk Score."""
import pytest
from med_risk_scores.engine import compute


class TestCha2ds2Vasc:
    """
    CHA₂DS₂-VASc scoring:
      C – CHF:                +1
      H – Hypertension:       +1
      A2 – Age ≥ 75:          +2
      D – Diabetes:           +1
      S2 – Stroke/TIA/TE:     +2
      V – Vascular disease:   +1
      A – Age 65-74:          +1
      Sc – Female sex:        +1
    Max = 9
    """

    def test_zero_risk(self):
        """No risk factors -> 0."""
        r = compute("cha2ds2_vasc", {
            "chf": False, "hypertension": False, "age": 50,
            "diabetes": False, "stroke_tia": False,
            "vascular_disease": False, "sex_female": False,
        })
        assert r.total_score == 0
        assert r.risk_label == "Low"

    def test_single_hypertension(self):
        """Only hypertension -> 1."""
        r = compute("cha2ds2_vasc", {
            "chf": False, "hypertension": True, "age": 50,
            "diabetes": False, "stroke_tia": False,
            "vascular_disease": False, "sex_female": False,
        })
        assert r.total_score == 1
        assert r.risk_label == "Low-Moderate"
        assert r.contributions["Hypertension"] == 1.0

    def test_age_75_gives_two_points(self):
        """Age ≥ 75 -> +2 for A2."""
        r = compute("cha2ds2_vasc", {
            "chf": False, "hypertension": False, "age": 80,
            "diabetes": False, "stroke_tia": False,
            "vascular_disease": False, "sex_female": False,
        })
        assert r.total_score == 2
        assert r.contributions["Age ≥ 75"] == 2.0

    def test_age_65_gives_one_point(self):
        """Age 65-74 -> +1 for A."""
        r = compute("cha2ds2_vasc", {
            "chf": False, "hypertension": False, "age": 68,
            "diabetes": False, "stroke_tia": False,
            "vascular_disease": False, "sex_female": False,
        })
        assert r.total_score == 1
        assert r.contributions["Age 65-74"] == 1.0

    def test_age_74_no_75_points(self):
        """Age 74 -> +1 (65-74), not +2 (75+)."""
        r = compute("cha2ds2_vasc", {
            "chf": False, "hypertension": False, "age": 74,
            "diabetes": False, "stroke_tia": False,
            "vascular_disease": False, "sex_female": False,
        })
        assert r.total_score == 1
        assert r.contributions["Age 65-74"] == 1.0
        assert r.contributions["Age ≥ 75"] == 0.0

    def test_textbook_female_72_htn_dm(self):
        """
        Textbook example: 72yo female, HTN + DM.
        Points: H=1, A(65-74)=1, D=1, Sc=1 -> 4 (High risk).
        """
        r = compute("cha2ds2_vasc", {
            "chf": False, "hypertension": True, "age": 72,
            "diabetes": True, "stroke_tia": False,
            "vascular_disease": False, "sex_female": True,
        })
        assert r.total_score == 4
        assert r.risk_label == "High"

    def test_max_score_all_factors(self):
        """All risk factors -> 9."""
        r = compute("cha2ds2_vasc", {
            "chf": True, "hypertension": True, "age": 80,
            "diabetes": True, "stroke_tia": True,
            "vascular_disease": True, "sex_female": True,
        })
        assert r.total_score == 9
        assert r.risk_label == "High"

    def test_stroke_gives_two_points(self):
        """Prior stroke/TIA -> +2."""
        r = compute("cha2ds2_vasc", {
            "chf": False, "hypertension": False, "age": 50,
            "diabetes": False, "stroke_tia": True,
            "vascular_disease": False, "sex_female": False,
        })
        assert r.total_score == 2
        assert r.contributions["Prior stroke/TIA/TE"] == 2.0

    def test_chf_gives_one_point(self):
        r = compute("cha2ds2_vasc", {
            "chf": True, "hypertension": False, "age": 50,
            "diabetes": False, "stroke_tia": False,
            "vascular_disease": False, "sex_female": False,
        })
        assert r.total_score == 1
        assert r.contributions["CHF/LV dysfunction"] == 1.0

    def test_vascular_disease_gives_one_point(self):
        r = compute("cha2ds2_vasc", {
            "chf": False, "hypertension": False, "age": 50,
            "diabetes": False, "stroke_tia": False,
            "vascular_disease": True, "sex_female": False,
        })
        assert r.total_score == 1
        assert r.contributions["Vascular disease"] == 1.0

    def test_female_only_young(self):
        """Young female alone -> 1 (sex only)."""
        r = compute("cha2ds2_vasc", {
            "chf": False, "hypertension": False, "age": 40,
            "diabetes": False, "stroke_tia": False,
            "vascular_disease": False, "sex_female": True,
        })
        assert r.total_score == 1
        assert r.risk_label == "Low-Moderate"

    def test_category_boundary_low_to_moderate(self):
        """Score 2 -> Moderate."""
        r = compute("cha2ds2_vasc", {
            "chf": True, "hypertension": True, "age": 50,
            "diabetes": False, "stroke_tia": False,
            "vascular_disease": False, "sex_female": False,
        })
        assert r.total_score == 2
        assert r.risk_label == "Moderate"

    def test_missing_input_raises(self):
        with pytest.raises(Exception):
            compute("cha2ds2_vasc", {"age": 70})
