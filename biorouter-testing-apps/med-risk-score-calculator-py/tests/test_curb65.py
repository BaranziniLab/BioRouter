"""Tests for CURB-65 Pneumonia Severity Score."""
import pytest
from med_risk_scores.engine import compute


class TestCurb65:
    """
    CURB-65:
      C – Confusion:          +1
      U – BUN ≥ 19 mg/dL:    +1
      R – RR ≥ 30:            +1
      B – SBP < 90 or DBP ≤ 60: +1
      65 – Age ≥ 65:           +1
    Max = 5
    """

    def test_zero_risk(self):
        """Young, stable patient, no confusion."""
        r = compute("curb65", {
            "confusion": False, "bun": 15, "respiratory_rate": 18,
            "systolic_bp": 130, "diastolic_bp": 80, "age": 45,
        })
        assert r.total_score == 0
        assert r.risk_label == "Low risk (0)"

    def test_confusion_only(self):
        r = compute("curb65", {
            "confusion": True, "bun": 15, "respiratory_rate": 18,
            "systolic_bp": 130, "diastolic_bp": 80, "age": 45,
        })
        assert r.total_score == 1
        assert r.risk_label == "Low risk (1)"

    def test_bun_boundary_19(self):
        """BUN at exactly 19 -> counts."""
        r = compute("curb65", {
            "confusion": False, "bun": 19, "respiratory_rate": 18,
            "systolic_bp": 130, "diastolic_bp": 80, "age": 45,
        })
        assert r.total_score == 1
        assert r.contributions["BUN ≥ 19 mg/dL"] == 1.0

    def test_bun_below_19(self):
        r = compute("curb65", {
            "confusion": False, "bun": 18, "respiratory_rate": 18,
            "systolic_bp": 130, "diastolic_bp": 80, "age": 45,
        })
        assert r.contributions["BUN ≥ 19 mg/dL"] == 0.0

    def test_rr_boundary_30(self):
        """RR at exactly 30 -> counts."""
        r = compute("curb65", {
            "confusion": False, "bun": 15, "respiratory_rate": 30,
            "systolic_bp": 130, "diastolic_bp": 80, "age": 45,
        })
        assert r.total_score == 1
        assert r.contributions["RR ≥ 30"] == 1.0

    def test_rr_29_no_points(self):
        r = compute("curb65", {
            "confusion": False, "bun": 15, "respiratory_rate": 29,
            "systolic_bp": 130, "diastolic_bp": 80, "age": 45,
        })
        assert r.contributions["RR ≥ 30"] == 0.0

    def test_low_sbp(self):
        """SBP < 90 -> counts."""
        r = compute("curb65", {
            "confusion": False, "bun": 15, "respiratory_rate": 18,
            "systolic_bp": 85, "diastolic_bp": 55, "age": 45,
        })
        assert r.contributions["BP < 90/60"] == 1.0

    def test_sbp_90_no_points(self):
        """SBP = 90 -> does not count (needs < 90)."""
        r = compute("curb65", {
            "confusion": False, "bun": 15, "respiratory_rate": 18,
            "systolic_bp": 90, "diastolic_bp": 80, "age": 45,
        })
        assert r.contributions["BP < 90/60"] == 0.0

    def test_low_dbp(self):
        """DBP ≤ 60 -> counts."""
        r = compute("curb65", {
            "confusion": False, "bun": 15, "respiratory_rate": 18,
            "systolic_bp": 130, "diastolic_bp": 60, "age": 45,
        })
        assert r.contributions["BP < 90/60"] == 1.0

    def test_age_boundary_65(self):
        """Age 65 -> counts."""
        r = compute("curb65", {
            "confusion": False, "bun": 15, "respiratory_rate": 18,
            "systolic_bp": 130, "diastolic_bp": 80, "age": 65,
        })
        assert r.total_score == 1
        assert r.contributions["Age ≥ 65"] == 1.0

    def test_age_64_no_points(self):
        r = compute("curb65", {
            "confusion": False, "bun": 15, "respiratory_rate": 18,
            "systolic_bp": 130, "diastolic_bp": 80, "age": 64,
        })
        assert r.contributions["Age ≥ 65"] == 0.0

    def test_all_positive(self):
        """All 5 -> very high risk."""
        r = compute("curb65", {
            "confusion": True, "bun": 40, "respiratory_rate": 35,
            "systolic_bp": 80, "diastolic_bp": 50, "age": 80,
        })
        assert r.total_score == 5
        assert r.risk_label == "Very high risk (4-5)"

    def test_moderate_two_factors(self):
        """Two factors -> moderate risk."""
        r = compute("curb65", {
            "confusion": True, "bun": 25, "respiratory_rate": 18,
            "systolic_bp": 130, "diastolic_bp": 80, "age": 45,
        })
        assert r.total_score == 2
        assert r.risk_label == "Moderate risk (2)"

    def test_high_three_factors(self):
        """3 factors -> high risk."""
        r = compute("curb65", {
            "confusion": True, "bun": 25, "respiratory_rate": 35,
            "systolic_bp": 130, "diastolic_bp": 80, "age": 45,
        })
        assert r.total_score == 3
        assert r.risk_label == "High risk (3)"

    def test_missing_input_raises(self):
        with pytest.raises(Exception):
            compute("curb65", {"confusion": True})

    def test_invalid_bun_negative(self):
        with pytest.raises(Exception):
            compute("curb65", {
                "confusion": False, "bun": -5, "respiratory_rate": 18,
                "systolic_bp": 130, "diastolic_bp": 80, "age": 45,
            })
