"""Tests for APACHE II-lite ICU Severity Score."""
import pytest
from med_risk_scores.engine import compute


class TestApacheIILite:
    def test_normal_physiology(self):
        """Normal values -> low APS."""
        r = compute("apache_ii_lite", {
            "temperature": 37.0, "mean_arterial_pressure": 85,
            "heart_rate": 78, "respiratory_rate": 16,
            "oxygenation": 95, "arterial_pH": 7.40,
            "sodium": 140, "potassium": 4.0, "creatinine": 1.0,
            "hematocrit": 40, "wbc": 10, "gcs": 15,
            "age": 40, "chronic_health": False,
        })
        assert r.total_score == 0

    def test_hypothermia_adds_points(self):
        """Temperature <= 29.9 -> +4."""
        r = compute("apache_ii_lite", {
            "temperature": 29.0, "mean_arterial_pressure": 85,
            "heart_rate": 78, "respiratory_rate": 16,
            "oxygenation": 95, "arterial_pH": 7.40,
            "sodium": 140, "potassium": 4.0, "creatinine": 1.0,
            "hematocrit": 40, "wbc": 10, "gcs": 15,
            "age": 40, "chronic_health": False,
        })
        assert r.contributions["Temperature"] == 4.0

    def test_hyperthermia_adds_points(self):
        """Temperature > 41.0 -> +4."""
        r = compute("apache_ii_lite", {
            "temperature": 41.5, "mean_arterial_pressure": 85,
            "heart_rate": 78, "respiratory_rate": 16,
            "oxygenation": 95, "arterial_pH": 7.40,
            "sodium": 140, "potassium": 4.0, "creatinine": 1.0,
            "hematocrit": 40, "wbc": 10, "gcs": 15,
            "age": 40, "chronic_health": False,
        })
        assert r.contributions["Temperature"] == 4.0

    def test_hypotension_high_aps(self):
        """MAP <= 49 -> +4."""
        r = compute("apache_ii_lite", {
            "temperature": 37.0, "mean_arterial_pressure": 45,
            "heart_rate": 78, "respiratory_rate": 16,
            "oxygenation": 95, "arterial_pH": 7.40,
            "sodium": 140, "potassium": 4.0, "creatinine": 1.0,
            "hematocrit": 40, "wbc": 10, "gcs": 15,
            "age": 40, "chronic_health": False,
        })
        assert r.contributions["MAP"] == 4.0

    def test_tachycardia(self):
        """HR > 179 -> +4."""
        r = compute("apache_ii_lite", {
            "temperature": 37.0, "mean_arterial_pressure": 85,
            "heart_rate": 180, "respiratory_rate": 16,
            "oxygenation": 95, "arterial_pH": 7.40,
            "sodium": 140, "potassium": 4.0, "creatinine": 1.0,
            "hematocrit": 40, "wbc": 10, "gcs": 15,
            "age": 40, "chronic_health": False,
        })
        assert r.contributions["Heart rate"] == 4.0

    def test_apnea(self):
        """RR <= 5 -> +4."""
        r = compute("apache_ii_lite", {
            "temperature": 37.0, "mean_arterial_pressure": 85,
            "heart_rate": 78, "respiratory_rate": 5,
            "oxygenation": 95, "arterial_pH": 7.40,
            "sodium": 140, "potassium": 4.0, "creatinine": 1.0,
            "hematocrit": 40, "wbc": 10, "gcs": 15,
            "age": 40, "chronic_health": False,
        })
        assert r.contributions["Respiratory rate"] == 4.0

    def test_low_ph(self):
        """pH < 7.15 -> +4."""
        r = compute("apache_ii_lite", {
            "temperature": 37.0, "mean_arterial_pressure": 85,
            "heart_rate": 78, "respiratory_rate": 16,
            "oxygenation": 95, "arterial_pH": 7.10,
            "sodium": 140, "potassium": 4.0, "creatinine": 1.0,
            "hematocrit": 40, "wbc": 10, "gcs": 15,
            "age": 40, "chronic_health": False,
        })
        assert r.contributions["Arterial pH"] == 4.0

    def test_hyponatremia(self):
        """Na < 120 -> +4."""
        r = compute("apache_ii_lite", {
            "temperature": 37.0, "mean_arterial_pressure": 85,
            "heart_rate": 78, "respiratory_rate": 16,
            "oxygenation": 95, "arterial_pH": 7.40,
            "sodium": 115, "potassium": 4.0, "creatinine": 1.0,
            "hematocrit": 40, "wbc": 10, "gcs": 15,
            "age": 40, "chronic_health": False,
        })
        assert r.contributions["Sodium"] == 4.0

    def test_hyperkalemia(self):
        """K >= 6.0 -> +4."""
        r = compute("apache_ii_lite", {
            "temperature": 37.0, "mean_arterial_pressure": 85,
            "heart_rate": 78, "respiratory_rate": 16,
            "oxygenation": 95, "arterial_pH": 7.40,
            "sodium": 140, "potassium": 6.5, "creatinine": 1.0,
            "hematocrit": 40, "wbc": 10, "gcs": 15,
            "age": 40, "chronic_health": False,
        })
        assert r.contributions["Potassium"] == 4.0

    def test_high_creatinine(self):
        """Cr >= 3.5 -> +4."""
        r = compute("apache_ii_lite", {
            "temperature": 37.0, "mean_arterial_pressure": 85,
            "heart_rate": 78, "respiratory_rate": 16,
            "oxygenation": 95, "arterial_pH": 7.40,
            "sodium": 140, "potassium": 4.0, "creatinine": 4.0,
            "hematocrit": 40, "wbc": 10, "gcs": 15,
            "age": 40, "chronic_health": False,
        })
        assert r.contributions["Creatinine"] == 4.0

    def test_low_hematocrit(self):
        """Hct < 20 -> +4."""
        r = compute("apache_ii_lite", {
            "temperature": 37.0, "mean_arterial_pressure": 85,
            "heart_rate": 78, "respiratory_rate": 16,
            "oxygenation": 95, "arterial_pH": 7.40,
            "sodium": 140, "potassium": 4.0, "creatinine": 1.0,
            "hematocrit": 18, "wbc": 10, "gcs": 15,
            "age": 40, "chronic_health": False,
        })
        assert r.contributions["Hematocrit"] == 4.0

    def test_leukopenia(self):
        """WBC < 1.0 -> +4."""
        r = compute("apache_ii_lite", {
            "temperature": 37.0, "mean_arterial_pressure": 85,
            "heart_rate": 78, "respiratory_rate": 16,
            "oxygenation": 95, "arterial_pH": 7.40,
            "sodium": 140, "potassium": 4.0, "creatinine": 1.0,
            "hematocrit": 40, "wbc": 0.5, "gcs": 15,
            "age": 40, "chronic_health": False,
        })
        assert r.contributions["WBC"] == 4.0

    def test_low_gcs(self):
        """GCS 3 -> 15-3 = 12 GCS points."""
        r = compute("apache_ii_lite", {
            "temperature": 37.0, "mean_arterial_pressure": 85,
            "heart_rate": 78, "respiratory_rate": 16,
            "oxygenation": 95, "arterial_pH": 7.40,
            "sodium": 140, "potassium": 4.0, "creatinine": 1.0,
            "hematocrit": 40, "wbc": 10, "gcs": 3,
            "age": 40, "chronic_health": False,
        })
        assert r.contributions["GCS points (15 - GCS)"] == 12.0

    def test_elderly_age_points(self):
        """Age 75+ -> +6."""
        r = compute("apache_ii_lite", {
            "temperature": 37.0, "mean_arterial_pressure": 85,
            "heart_rate": 78, "respiratory_rate": 16,
            "oxygenation": 95, "arterial_pH": 7.40,
            "sodium": 140, "potassium": 4.0, "creatinine": 1.0,
            "hematocrit": 40, "wbc": 10, "gcs": 15,
            "age": 76, "chronic_health": False,
        })
        assert r.contributions["Age points"] == 6.0

    def test_young_age_zero(self):
        """Age < 45 -> 0."""
        r = compute("apache_ii_lite", {
            "temperature": 37.0, "mean_arterial_pressure": 85,
            "heart_rate": 78, "respiratory_rate": 16,
            "oxygenation": 95, "arterial_pH": 7.40,
            "sodium": 140, "potassium": 4.0, "creatinine": 1.0,
            "hematocrit": 40, "wbc": 10, "gcs": 15,
            "age": 35, "chronic_health": False,
        })
        assert r.contributions["Age points"] == 0.0

    def test_chronic_health_adds_five(self):
        r = compute("apache_ii_lite", {
            "temperature": 37.0, "mean_arterial_pressure": 85,
            "heart_rate": 78, "respiratory_rate": 16,
            "oxygenation": 95, "arterial_pH": 7.40,
            "sodium": 140, "potassium": 4.0, "creatinine": 1.0,
            "hematocrit": 40, "wbc": 10, "gcs": 15,
            "age": 40, "chronic_health": True,
        })
        assert r.contributions["Chronic health points"] == 5.0

    def test_sick_patient_score(self):
        """Multi-system derangement."""
        r = compute("apache_ii_lite", {
            "temperature": 29.0, "mean_arterial_pressure": 45,
            "heart_rate": 180, "respiratory_rate": 5,
            "oxygenation": 45, "arterial_pH": 7.10,
            "sodium": 115, "potassium": 7.0, "creatinine": 5.0,
            "hematocrit": 18, "wbc": 0.5, "gcs": 3,
            "age": 80, "chronic_health": True,
        })
        assert r.total_score >= 50
        assert r.risk_label == "Very severe illness"

    def test_result_has_all_contributions(self):
        r = compute("apache_ii_lite", {
            "temperature": 37.0, "mean_arterial_pressure": 85,
            "heart_rate": 78, "respiratory_rate": 16,
            "oxygenation": 95, "arterial_pH": 7.40,
            "sodium": 140, "potassium": 4.0, "creatinine": 1.0,
            "hematocrit": 40, "wbc": 10, "gcs": 15,
            "age": 40, "chronic_health": False,
        })
        # Should have 12 physiology + age + chronic = 14 contribution keys
        assert len(r.contributions) == 14

    def test_missing_inputs_raises(self):
        with pytest.raises(Exception):
            compute("apache_ii_lite", {
                "temperature": 37.0, "mean_arterial_pressure": 85,
                "heart_rate": 78,
            })
