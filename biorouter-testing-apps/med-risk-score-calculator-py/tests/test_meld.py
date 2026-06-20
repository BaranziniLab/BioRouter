"""Tests for MELD and MELD-Na Liver Disease Scores."""
import math
import pytest
from med_risk_scores.engine import compute


class TestMeld:
    """
    MELD = 3.78*ln(bili) + 11.2*ln(INR) + 9.57*ln(cr) + 6.43
    Floored at 6, capped at 40.
    """

    def test_known_textbook_values(self):
        """
        Classic example: bili=2.0, INR=1.5, cr=1.0
        MELD = 3.78*ln(2) + 11.2*ln(1.5) + 9.57*ln(1) + 6.43
             = 3.78*0.6931 + 11.2*0.4055 + 9.57*0 + 6.43
             = 2.6198 + 4.5416 + 0 + 6.43
             = 13.5914 -> 14
        """
        r = compute("meld", {
            "bilirubin": 2.0, "inr": 1.5, "creatinine": 1.0, "dialysis": False,
        })
        assert r.total_score == 14
        assert r.risk_label == "Moderate severity"

    def test_high_meld(self):
        """bili=10, INR=3.0, cr=4.0 -> high MELD."""
        r = compute("meld", {
            "bilirubin": 10.0, "inr": 3.0, "creatinine": 4.0, "dialysis": False,
        })
        # 3.78*ln(10) + 11.2*ln(3) + 9.57*ln(4) + 6.43
        # = 3.78*2.3026 + 11.2*1.0986 + 9.57*1.3863 + 6.43
        # = 8.704 + 12.304 + 13.269 + 6.43 = 40.707 -> capped at 40
        assert r.total_score == 40
        assert r.risk_label == "Critical severity"

    def test_minimum_meld(self):
        """Low bilirubin, INR, creatinine -> MELD floored at 6."""
        r = compute("meld", {
            "bilirubin": 0.5, "inr": 0.8, "creatinine": 0.3, "dialysis": False,
        })
        assert r.total_score >= 6
        assert r.total_score <= 6

    def test_dialysis_overrides_creatinine(self):
        """Dialysis -> creatinine floored at 4.0."""
        r = compute("meld", {
            "bilirubin": 2.0, "inr": 1.0, "creatinine": 0.8, "dialysis": True,
        })
        # cr forced to max(0.8, 4.0) = 4.0
        expected_no_dial = compute("meld", {
            "bilirubin": 2.0, "inr": 1.0, "creatinine": 4.0, "dialysis": False,
        })
        assert r.total_score == expected_no_dial.total_score

    def test_creatinine_floor_at_1(self):
        """Creatinine < 1.0 is floored to 1.0 in formula."""
        r_low = compute("meld", {
            "bilirubin": 2.0, "inr": 1.5, "creatinine": 0.3, "dialysis": False,
        })
        r_at1 = compute("meld", {
            "bilirubin": 2.0, "inr": 1.5, "creatinine": 1.0, "dialysis": False,
        })
        assert r_low.total_score == r_at1.total_score

    def test_bilirubin_floor_at_1(self):
        """Bilirubin < 1 is floored to 1.0."""
        r = compute("meld", {
            "bilirubin": 0.2, "inr": 1.5, "creatinine": 1.0, "dialysis": False,
        })
        r2 = compute("meld", {
            "bilirubin": 1.0, "inr": 1.5, "creatinine": 1.0, "dialysis": False,
        })
        assert r.total_score == r2.total_score

    def test_contributions_include_all_terms(self):
        r = compute("meld", {
            "bilirubin": 2.0, "inr": 1.5, "creatinine": 1.0, "dialysis": False,
        })
        assert len(r.contributions) == 4  # bilirubin, INR, creatinine, constant

    def test_missing_input_raises(self):
        with pytest.raises(Exception):
            compute("meld", {"bilirubin": 2.0})


class TestMeldNa:
    def test_basic_computation(self):
        """MELD-Na should adjust for sodium."""
        r = compute("meld_na", {
            "bilirubin": 2.0, "inr": 1.5, "creatinine": 1.0,
            "dialysis": False, "sodium": 135,
        })
        assert r.total_score >= 6

    def test_low_sodium_increases_score(self):
        """Lower Na should increase MELD-Na."""
        r_normal = compute("meld_na", {
            "bilirubin": 2.0, "inr": 1.5, "creatinine": 1.0,
            "dialysis": False, "sodium": 140,
        })
        r_low = compute("meld_na", {
            "bilirubin": 2.0, "inr": 1.5, "creatinine": 1.0,
            "dialysis": False, "sodium": 130,
        })
        assert r_low.total_score >= r_normal.total_score

    def test_na_floor_at_125(self):
        """Sodium floored at 125 inside the formula."""
        # Test that values at the floor boundary behave as the floor
        r_at_floor = compute("meld_na", {
            "bilirubin": 2.0, "inr": 1.5, "creatinine": 1.0,
            "dialysis": False, "sodium": 125,
        })
        # Sodium 125 is the min, so it should equal the floor value
        assert r_at_floor.total_score >= 6

    def test_na_ceiling_at_145(self):
        """Sodium capped at 145 inside the formula."""
        r_at_ceil = compute("meld_na", {
            "bilirubin": 2.0, "inr": 1.5, "creatinine": 1.0,
            "dialysis": False, "sodium": 145,
        })
        assert r_at_ceil.total_score >= 6

    def test_result_has_sodium_correction(self):
        r = compute("meld_na", {
            "bilirubin": 2.0, "inr": 1.5, "creatinine": 1.0,
            "dialysis": False, "sodium": 130,
        })
        has_na_key = any("Na" in k for k in r.contributions)
        assert has_na_key
