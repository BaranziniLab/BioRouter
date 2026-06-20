"""Tests for qSOFA Sepsis Screening Score."""
import pytest
from med_risk_scores.engine import compute


class TestQsofa:
    """
    qSOFA:
      RR ≥ 22:            +1
      Altered mentation:  +1
      SBP ≤ 100:          +1
    Max = 3
    Score >= 2 suggests sepsis with organ dysfunction.
    """

    def test_no_risk_factors(self):
        r = compute("qsofa", {
            "respiratory_rate": 18, "altered_mentation": False,
            "systolic_bp": 130,
        })
        assert r.total_score == 0
        assert r.risk_label == "Low risk"

    def test_rr_only(self):
        """RR >= 22 alone -> 1."""
        r = compute("qsofa", {
            "respiratory_rate": 22, "altered_mentation": False,
            "systolic_bp": 130,
        })
        assert r.total_score == 1
        assert r.risk_label == "Low risk"

    def test_rr_boundary_21(self):
        """RR = 21 -> 0."""
        r = compute("qsofa", {
            "respiratory_rate": 21, "altered_mentation": False,
            "systolic_bp": 130,
        })
        assert r.contributions["Respiratory rate ≥ 22"] == 0.0

    def test_rr_boundary_22(self):
        """RR = 22 -> 1."""
        r = compute("qsofa", {
            "respiratory_rate": 22, "altered_mentation": False,
            "systolic_bp": 130,
        })
        assert r.contributions["Respiratory rate ≥ 22"] == 1.0

    def test_altered_mentation_only(self):
        r = compute("qsofa", {
            "respiratory_rate": 18, "altered_mentation": True,
            "systolic_bp": 130,
        })
        assert r.total_score == 1

    def test_sbp_low_only(self):
        """SBP <= 100 -> 1."""
        r = compute("qsofa", {
            "respiratory_rate": 18, "altered_mentation": False,
            "systolic_bp": 100,
        })
        assert r.total_score == 1
        assert r.contributions["Systolic BP ≤ 100"] == 1.0

    def test_sbp_101_no_points(self):
        r = compute("qsofa", {
            "respiratory_rate": 18, "altered_mentation": False,
            "systolic_bp": 101,
        })
        assert r.contributions["Systolic BP ≤ 100"] == 0.0

    def test_two_factors_high_risk(self):
        """RR + hypotension -> 2 -> high risk."""
        r = compute("qsofa", {
            "respiratory_rate": 25, "altered_mentation": False,
            "systolic_bp": 90,
        })
        assert r.total_score == 2
        assert r.risk_label == "High risk"

    def test_three_factors_max(self):
        """All three -> 3 -> high risk."""
        r = compute("qsofa", {
            "respiratory_rate": 30, "altered_mentation": True,
            "systolic_bp": 80,
        })
        assert r.total_score == 3
        assert r.risk_label == "High risk"

    def test_interpretation_mentions_sepsis(self):
        r = compute("qsofa", {
            "respiratory_rate": 25, "altered_mentation": True,
            "systolic_bp": 85,
        })
        assert "sepsis" in r.interpretation.lower()

    def test_interpretation_for_low_score(self):
        r = compute("qsofa", {
            "respiratory_rate": 16, "altered_mentation": False,
            "systolic_bp": 130,
        })
        assert "standard care" in r.interpretation.lower() or "unlikely" in r.interpretation.lower()

    def test_missing_inputs_raises(self):
        with pytest.raises(Exception):
            compute("qsofa", {})
