"""Tests for the score registry and computation engine."""
import pytest
from med_risk_scores.registry import (
    list_scores,
    get_score,
    all_definitions,
    ScoreResult,
    RiskCategory,
    register_score,
    VariableSpec,
    _REGISTRY,
)
from med_risk_scores.engine import compute, compute_from_definition, compute_safe


class TestRegistry:
    def test_list_scores_not_empty(self):
        scores = list_scores()
        assert len(scores) >= 11
        assert "cha2ds2_vasc" in scores
        assert "has_bled" in scores
        assert "wells_dvt" in scores
        assert "wells_pe" in scores
        assert "curb65" in scores
        assert "meld" in scores
        assert "meld_na" in scores
        assert "qsofa" in scores
        assert "framingham_risk_score" in scores
        assert "ascvd_10yr" in scores
        assert "apache_ii_lite" in scores

    def test_get_score_returns_definition(self):
        defn = get_score("cha2ds2_vasc")
        assert defn.name == "cha2ds2_vasc"
        assert defn.display_name == "CHA₂DS₂-VASc"
        assert len(defn.variables) > 0
        assert len(defn.categories) > 0

    def test_get_score_case_insensitive(self):
        d1 = get_score("CHA2DS2_VASC")
        d2 = get_score("cha2ds2_vasc")
        assert d1.name == d2.name

    def test_get_score_hyphen_to_underscore(self):
        d = get_score("cha2ds2-vasc")
        assert d.name == "cha2ds2_vasc"

    def test_get_score_unknown_raises(self):
        with pytest.raises(KeyError, match="Unknown score"):
            get_score("nonexistent_score_xyz")

    def test_all_definitions(self):
        defs = all_definitions()
        assert isinstance(defs, dict)
        assert "cha2ds2_vasc" in defs

    def test_duplicate_registration_raises(self):
        with pytest.raises(ValueError, match="already registered"):
            register_score(
                name="cha2ds2_vasc",
                display_name="Duplicate",
                description="Should fail",
                variables=[],
                compute_fn=lambda x: (0, {}),
                categories=[],
            )

    def test_score_classify(self):
        defn = get_score("cha2ds2_vasc")
        cat_low = defn.classify(0)
        cat_high = defn.classify(6)
        assert cat_low.label == "Low"
        assert cat_high.label == "High"

    def test_score_variable_names(self):
        defn = get_score("cha2ds2_vasc")
        names = defn.variable_names
        assert "age" in names
        assert "chf" in names
        assert "diabetes" in names


class TestEngineCompute:
    def test_cha2ds2_vasc_known_value(self):
        """72yo female with HTN and DM -> score 4 (High)."""
        r = compute("cha2ds2_vasc", {
            "chf": False, "hypertension": True, "age": 72,
            "diabetes": True, "stroke_tia": False,
            "vascular_disease": False, "sex_female": True,
        })
        assert r.total_score == 4.0
        assert r.risk_label == "High"
        assert "anticoagulation" in r.interpretation.lower()

    def test_cha2ds2_vasc_zero(self):
        r = compute("cha2ds2_vasc", {
            "chf": False, "hypertension": False, "age": 50,
            "diabetes": False, "stroke_tia": False,
            "vascular_disease": False, "sex_female": False,
        })
        assert r.total_score == 0.0
        assert r.risk_label == "Low"

    def test_validation_error_on_missing(self):
        with pytest.raises(Exception):
            compute("cha2ds2_vasc", {"age": 70})

    def test_validation_error_on_bad_type(self):
        with pytest.raises(Exception):
            compute("curb65", {"confusion": "yes", "bun": "not_a_number",
                               "respiratory_rate": 20, "systolic_bp": 120,
                               "diastolic_bp": 80, "age": 65})

    def test_result_is_score_result(self):
        r = compute("qsofa", {
            "respiratory_rate": 25, "altered_mentation": True,
            "systolic_bp": 90,
        })
        assert isinstance(r, ScoreResult)
        assert hasattr(r, "total_score")
        assert hasattr(r, "to_dict")

    def test_result_to_dict(self):
        r = compute("qsofa", {
            "respiratory_rate": 25, "altered_mentation": True,
            "systolic_bp": 90,
        })
        d = r.to_dict()
        assert d["score_name"] == "qsofa"
        assert d["total_score"] == 3.0
        assert "contributions" in d


class TestComputeSafe:
    def test_success(self):
        result = compute_safe("qsofa", {
            "respiratory_rate": 25, "altered_mentation": True,
            "systolic_bp": 90,
        })
        assert result["ok"] is True
        assert result["result"]["total_score"] == 3.0

    def test_validation_failure(self):
        result = compute_safe("cha2ds2_vasc", {})
        assert result["ok"] is False
        assert len(result["errors"]) > 0

    def test_unknown_score(self):
        result = compute_safe("nonexistent", {})
        assert result["ok"] is False
