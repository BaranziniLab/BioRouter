"""Tests for Wells DVT and Wells PE Scores."""
import pytest
from med_risk_scores.engine import compute


class TestWellsDVT:
    """
    Wells DVT:
      Each criterion except alternative_diagnosis = +1
      Alternative diagnosis = -2
    """

    def test_no_factors(self):
        r = compute("wells_dvt", {
            "active_cancer": False, "paralysis": False,
            "bedridden": False, "localized_tenderness": False,
            "entire_leg_swollen": False, "calf_swelling": False,
            "pitting_edema": False, "collateral_veins": False,
            "alternative_diagnosis": False,
        })
        assert r.total_score == 0
        assert r.risk_label == "Low probability"

    def test_active_cancer(self):
        r = compute("wells_dvt", {
            "active_cancer": True, "paralysis": False,
            "bedridden": False, "localized_tenderness": False,
            "entire_leg_swollen": False, "calf_swelling": False,
            "pitting_edema": False, "collateral_veins": False,
            "alternative_diagnosis": False,
        })
        assert r.total_score == 1
        assert r.risk_label == "Low probability"

    def test_two_factors_moderate(self):
        """Two positive factors -> 2 -> moderate."""
        r = compute("wells_dvt", {
            "active_cancer": True, "paralysis": True,
            "bedridden": False, "localized_tenderness": False,
            "entire_leg_swollen": False, "calf_swelling": False,
            "pitting_edema": False, "collateral_veins": False,
            "alternative_diagnosis": False,
        })
        assert r.total_score == 2
        assert r.risk_label == "Moderate probability"

    def test_four_factors_high(self):
        """4+ factors -> high probability."""
        r = compute("wells_dvt", {
            "active_cancer": True, "paralysis": True,
            "bedridden": True, "localized_tenderness": True,
            "entire_leg_swollen": False, "calf_swelling": False,
            "pitting_edema": False, "collateral_veins": False,
            "alternative_diagnosis": False,
        })
        assert r.total_score == 4
        assert r.risk_label == "High probability"

    def test_alternative_diagnosis_subtracts_two(self):
        r = compute("wells_dvt", {
            "active_cancer": True, "paralysis": True,
            "bedridden": True, "localized_tenderness": True,
            "entire_leg_swollen": False, "calf_swelling": False,
            "pitting_edema": False, "collateral_veins": False,
            "alternative_diagnosis": True,
        })
        assert r.total_score == 2  # 4 - 2
        assert r.contributions["Alternative diagnosis"] == -2.0

    def test_all_positive(self):
        r = compute("wells_dvt", {
            "active_cancer": True, "paralysis": True,
            "bedridden": True, "localized_tenderness": True,
            "entire_leg_swollen": True, "calf_swelling": True,
            "pitting_edema": True, "collateral_veins": True,
            "alternative_diagnosis": False,
        })
        assert r.total_score == 8
        assert r.risk_label == "High probability"

    def test_missing_inputs_raises(self):
        with pytest.raises(Exception):
            compute("wells_dvt", {"active_cancer": True})


class TestWellsPE:
    """
    Wells PE:
      DVT symptoms:       +3
      PE #1 diagnosis:    +3
      HR > 100:           +1.5
      Immobilisation:     +1.5
      Prior PE/DVT:       +1.5
      Hemoptysis:         +1.0
      Malignancy:         +1.0
    """

    def test_no_factors(self):
        r = compute("wells_pe", {
            "dvt_symptoms": False, "pe_number1": False,
            "heart_rate": 80, "immobilization": False,
            "prior_pe_dvt": False, "hemoptysis": False,
            "malignancy": False,
        })
        assert r.total_score == 0
        assert r.risk_label == "Low probability"

    def test_dvt_symptoms(self):
        r = compute("wells_pe", {
            "dvt_symptoms": True, "pe_number1": False,
            "heart_rate": 80, "immobilization": False,
            "prior_pe_dvt": False, "hemoptysis": False,
            "malignancy": False,
        })
        assert r.total_score == 3
        assert r.risk_label == "Moderate probability"

    def test_pe_number_one_diagnosis(self):
        r = compute("wells_pe", {
            "dvt_symptoms": False, "pe_number1": True,
            "heart_rate": 80, "immobilization": False,
            "prior_pe_dvt": False, "hemoptysis": False,
            "malignancy": False,
        })
        assert r.total_score == 3

    def test_hr_above_100(self):
        r = compute("wells_pe", {
            "dvt_symptoms": False, "pe_number1": False,
            "heart_rate": 110, "immobilization": False,
            "prior_pe_dvt": False, "hemoptysis": False,
            "malignancy": False,
        })
        assert r.total_score == 1.5

    def test_hr_at_100_no_points(self):
        r = compute("wells_pe", {
            "dvt_symptoms": False, "pe_number1": False,
            "heart_rate": 100, "immobilization": False,
            "prior_pe_dvt": False, "hemoptysis": False,
            "malignancy": False,
        })
        assert r.total_score == 0

    def test_classic_high_risk_patient(self):
        """
        DVT symptoms + PE #1 + tachycardia + prior PE -> 3+3+1.5+1.5 = 9
        """
        r = compute("wells_pe", {
            "dvt_symptoms": True, "pe_number1": True,
            "heart_rate": 120, "immobilization": False,
            "prior_pe_dvt": True, "hemoptysis": False,
            "malignancy": False,
        })
        assert r.total_score == 9
        assert r.risk_label == "High probability"

    def test_all_factors(self):
        r = compute("wells_pe", {
            "dvt_symptoms": True, "pe_number1": True,
            "heart_rate": 130, "immobilization": True,
            "prior_pe_dvt": True, "hemoptysis": True,
            "malignancy": True,
        })
        # 3+3+1.5+1.5+1.5+1+1 = 12.5
        assert r.total_score == 12.5
        assert r.risk_label == "High probability"

    def test_malignancy_and_hemoptysis(self):
        r = compute("wells_pe", {
            "dvt_symptoms": False, "pe_number1": False,
            "heart_rate": 80, "immobilization": False,
            "prior_pe_dvt": False, "hemoptysis": True,
            "malignancy": True,
        })
        assert r.total_score == 2.0
        assert r.risk_label == "Moderate probability"

    def test_missing_inputs_raises(self):
        with pytest.raises(Exception):
            compute("wells_pe", {})
