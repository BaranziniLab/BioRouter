"""Tests for Framingham Risk Score and ASCVD."""
import pytest
from med_risk_scores.engine import compute


class TestFraminghamRiskScore:
    def test_zero_risk_young_male(self):
        """20yo male, low TC, high HDL, low BP, non-smoker -> minimal points."""
        r = compute("framingham_risk_score", {
            "sex": "male", "age": 25, "total_cholesterol": 160,
            "hdl_cholesterol": 65, "systolic_bp": 115,
            "bp_treated": False, "smoker": False, "diabetes": False,
        })
        assert r.total_score <= 0

    def test_high_risk_smoker_male(self):
        """60yo male smoker, high TC, low HDL, elevated BP."""
        r = compute("framingham_risk_score", {
            "sex": "male", "age": 63, "total_cholesterol": 280,
            "hdl_cholesterol": 35, "systolic_bp": 160,
            "bp_treated": False, "smoker": True, "diabetes": False,
        })
        assert r.total_score >= 15

    def test_female_higher_age_points(self):
        """Same age, female gets more points than male."""
        r_male = compute("framingham_risk_score", {
            "sex": "male", "age": 55, "total_cholesterol": 220,
            "hdl_cholesterol": 50, "systolic_bp": 130,
            "bp_treated": False, "smoker": False, "diabetes": False,
        })
        r_female = compute("framingham_risk_score", {
            "sex": "female", "age": 55, "total_cholesterol": 220,
            "hdl_cholesterol": 50, "systolic_bp": 130,
            "bp_treated": False, "smoker": False, "diabetes": False,
        })
        # Females generally get more age + BP points
        assert r_female.total_score >= r_male.total_score

    def test_high_hdl_is_protective(self):
        """HDL >= 60 -> -1 point."""
        r = compute("framingham_risk_score", {
            "sex": "male", "age": 50, "total_cholesterol": 220,
            "hdl_cholesterol": 65, "systolic_bp": 130,
            "bp_treated": False, "smoker": False, "diabetes": False,
        })
        r_low = compute("framingham_risk_score", {
            "sex": "male", "age": 50, "total_cholesterol": 220,
            "hdl_cholesterol": 35, "systolic_bp": 130,
            "bp_treated": False, "smoker": False, "diabetes": False,
        })
        assert r.total_score < r_low.total_score

    def test_smoking_adds_points(self):
        r_smoke = compute("framingham_risk_score", {
            "sex": "male", "age": 50, "total_cholesterol": 220,
            "hdl_cholesterol": 50, "systolic_bp": 130,
            "bp_treated": False, "smoker": True, "diabetes": False,
        })
        r_nosmoke = compute("framingham_risk_score", {
            "sex": "male", "age": 50, "total_cholesterol": 220,
            "hdl_cholesterol": 50, "systolic_bp": 130,
            "bp_treated": False, "smoker": False, "diabetes": False,
        })
        assert r_smoke.total_score > r_nosmoke.total_score

    def test_diabetes_male_adds_two(self):
        """Diabetes adds 2 pts for males."""
        r_dm = compute("framingham_risk_score", {
            "sex": "male", "age": 50, "total_cholesterol": 220,
            "hdl_cholesterol": 50, "systolic_bp": 130,
            "bp_treated": False, "smoker": False, "diabetes": True,
        })
        r_no = compute("framingham_risk_score", {
            "sex": "male", "age": 50, "total_cholesterol": 220,
            "hdl_cholesterol": 50, "systolic_bp": 130,
            "bp_treated": False, "smoker": False, "diabetes": False,
        })
        assert r_dm.total_score == r_no.total_score + 2

    def test_diabetes_female_adds_three(self):
        """Diabetes adds 3 pts for females."""
        r_dm = compute("framingham_risk_score", {
            "sex": "female", "age": 50, "total_cholesterol": 220,
            "hdl_cholesterol": 50, "systolic_bp": 130,
            "bp_treated": False, "smoker": False, "diabetes": True,
        })
        r_no = compute("framingham_risk_score", {
            "sex": "female", "age": 50, "total_cholesterol": 220,
            "hdl_cholesterol": 50, "systolic_bp": 130,
            "bp_treated": False, "smoker": False, "diabetes": False,
        })
        assert r_dm.total_score == r_no.total_score + 3

    def test_treatment_increases_bp_points(self):
        """Treated BP gives more points."""
        r_treat = compute("framingham_risk_score", {
            "sex": "male", "age": 50, "total_cholesterol": 220,
            "hdl_cholesterol": 50, "systolic_bp": 140,
            "bp_treated": True, "smoker": False, "diabetes": False,
        })
        r_notreat = compute("framingham_risk_score", {
            "sex": "male", "age": 50, "total_cholesterol": 220,
            "hdl_cholesterol": 50, "systolic_bp": 140,
            "bp_treated": False, "smoker": False, "diabetes": False,
        })
        assert r_treat.total_score >= r_notreat.total_score

    def test_contributions_include_risk_pct(self):
        r = compute("framingham_risk_score", {
            "sex": "male", "age": 55, "total_cholesterol": 250,
            "hdl_cholesterol": 40, "systolic_bp": 155,
            "bp_treated": False, "smoker": True, "diabetes": False,
        })
        assert "Estimated 10-year CHD risk (%)" in r.contributions
        assert r.contributions["Estimated 10-year CHD risk (%)"] > 0

    def test_invalid_sex_raises(self):
        with pytest.raises(Exception):
            compute("framingham_risk_score", {
                "sex": "other", "age": 50, "total_cholesterol": 200,
                "hdl_cholesterol": 50, "systolic_bp": 130,
                "bp_treated": False, "smoker": False, "diabetes": False,
            })


class TestASCVD10yr:
    def test_basic_computation(self):
        """55yo white male, moderate risk factors."""
        r = compute("ascvd_10yr", {
            "sex": "male", "race": "white", "age": 55,
            "total_cholesterol": 210, "hdl_cholesterol": 45,
            "systolic_bp": 140, "bp_treated": False,
            "smoker": False, "diabetes": False,
        })
        assert r.total_score > 0
        assert r.total_score < 100

    def test_smoking_increases_risk(self):
        r_smoke = compute("ascvd_10yr", {
            "sex": "male", "race": "white", "age": 55,
            "total_cholesterol": 210, "hdl_cholesterol": 45,
            "systolic_bp": 140, "bp_treated": False,
            "smoker": True, "diabetes": False,
        })
        r_nosmoke = compute("ascvd_10yr", {
            "sex": "male", "race": "white", "age": 55,
            "total_cholesterol": 210, "hdl_cholesterol": 45,
            "systolic_bp": 140, "bp_treated": False,
            "smoker": False, "diabetes": False,
        })
        assert r_smoke.total_score > r_nosmoke.total_score

    def test_diabetes_increases_risk(self):
        r_dm = compute("ascvd_10yr", {
            "sex": "male", "race": "white", "age": 55,
            "total_cholesterol": 210, "hdl_cholesterol": 45,
            "systolic_bp": 140, "bp_treated": False,
            "smoker": False, "diabetes": True,
        })
        r_no = compute("ascvd_10yr", {
            "sex": "male", "race": "white", "age": 55,
            "total_cholesterol": 210, "hdl_cholesterol": 45,
            "systolic_bp": 140, "bp_treated": False,
            "smoker": False, "diabetes": False,
        })
        assert r_dm.total_score > r_no.total_score

    def test_african_american_male(self):
        """Different coefficient set should still compute."""
        r = compute("ascvd_10yr", {
            "sex": "male", "race": "african_american", "age": 55,
            "total_cholesterol": 210, "hdl_cholesterol": 45,
            "systolic_bp": 140, "bp_treated": False,
            "smoker": False, "diabetes": False,
        })
        assert r.total_score > 0

    def test_older_age_higher_risk(self):
        r_young = compute("ascvd_10yr", {
            "sex": "male", "race": "white", "age": 45,
            "total_cholesterol": 210, "hdl_cholesterol": 45,
            "systolic_bp": 140, "bp_treated": False,
            "smoker": False, "diabetes": False,
        })
        r_old = compute("ascvd_10yr", {
            "sex": "male", "race": "white", "age": 75,
            "total_cholesterol": 210, "hdl_cholesterol": 45,
            "systolic_bp": 140, "bp_treated": False,
            "smoker": False, "diabetes": False,
        })
        assert r_old.total_score > r_young.total_score

    def test_contributions_include_risk_pct(self):
        r = compute("ascvd_10yr", {
            "sex": "male", "race": "white", "age": 55,
            "total_cholesterol": 210, "hdl_cholesterol": 45,
            "systolic_bp": 140, "bp_treated": False,
            "smoker": True, "diabetes": True,
        })
        assert "10-year ASCVD risk (%)" in r.contributions
