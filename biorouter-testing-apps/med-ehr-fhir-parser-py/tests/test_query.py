"""
Tests for query.py — Query engine correctness.
"""

import pytest
from datetime import date, datetime

from fhir_parser.bundle import BundleFHIR
from fhir_parser.query import (
    query_active_conditions,
    query_latest_vitals,
    query_medications_on_date,
    query_observation_trends,
    query_allergy_intolerances,
    query_encounters,
    query_procedures,
    is_vital_sign,
)
from fhir_parser.resources import Observation, Condition, MedicationRequest, Patient
from fhir_parser.synthetic import (
    generate_patient_bundle,
    generate_simple_bundle,
    generate_observation,
    generate_condition,
    generate_medication_request,
)


class TestQueryActiveConditions:
    def test_returns_active_only(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_active_conditions(bundle)
        # We have 3 active + 1 resolved
        assert len(results) == 3
        for r in results:
            assert r.clinical_status in ("active", "recurrence", "relapse")

    def test_excludes_resolved(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_active_conditions(bundle)
        codes = [r.code_display for r in results]
        # Pneumonia is resolved, should not appear
        assert all("pneumonia" not in c.lower() for c in codes)

    def test_sorted_by_onset(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_active_conditions(bundle)
        dates = [r.onset_date for r in results if r.onset_date]
        assert dates == sorted(dates)

    def test_empty_bundle(self):
        raw = generate_simple_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_active_conditions(bundle)
        assert len(results) == 0

    def test_result_fields(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_active_conditions(bundle)
        for r in results:
            assert r.code_display
            assert r.clinical_status
            assert r.raw is not None

    def test_result_repr(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_active_conditions(bundle)
        assert len(results) > 0
        assert "ActiveCondition" in repr(results[0])

    def test_custom_conditions(self):
        """Build a bundle with custom conditions to test filtering."""
        patient = Patient(id="p1", resourceType="Patient", gender="female")
        cond_active = Condition(
            id="c1", resourceType="Condition",
            clinicalStatus=None,
            verificationStatus=None,
            code=None,
            subject=None,
        )
        # Manually set clinical status
        from fhir_parser.resources import CodeableConcept, Coding
        cond_active.clinicalStatus = CodeableConcept(
            coding=[Coding(code="active")]
        )
        cond_active.code = CodeableConcept(
            coding=[Coding(code="123", display="Test condition")]
        )

        cond_resolved = Condition(
            id="c2", resourceType="Condition",
            clinicalStatus=CodeableConcept(
                coding=[Coding(code="resolved")]
            ),
            code=CodeableConcept(
                coding=[Coding(code="456", display="Old condition")]
            ),
            subject=None,
        )

        bundle = BundleFHIR.from_resource_list([patient, cond_active, cond_resolved])
        results = query_active_conditions(bundle)
        assert len(results) == 1
        assert results[0].code_display == "Test condition"


class TestQueryLatestVitals:
    def test_returns_vital_signs(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_latest_vitals(bundle)
        assert len(results) > 0
        for r in results:
            assert r.code_display
            assert r.value

    def test_returns_latest_per_code(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_latest_vitals(bundle)
        # For each code, we should have at most one result (the latest)
        code_counts = {}
        for r in results:
            key = r.code_display
            code_counts[key] = code_counts.get(key, 0) + 1
        for code, count in code_counts.items():
            assert count == 1, f"Multiple results for {code}: {count}"

    def test_heart_rate_latest_is_highest_date(self):
        """Heart rate observations: 72 (Jan), 68 (Mar), 95 (Jun). Latest should be Jun."""
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_latest_vitals(bundle)
        hr = [r for r in results if "heart rate" in r.code_display.lower()]
        assert len(hr) == 1
        assert hr[0].numeric_value == 95.0  # The June value

    def test_with_code_filter(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_latest_vitals(bundle, codes={"8867-4"})
        for r in results:
            assert "heart rate" in r.code_display.lower()

    def test_result_repr(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_latest_vitals(bundle)
        if results:
            assert "LatestVital" in repr(results[0])


class TestQueryMedicationsOnDate:
    def test_active_medications_on_date(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        # Query medications active on 2024-06-01
        results = query_medications_on_date(bundle, date(2024, 6, 1))
        assert len(results) > 0
        for r in results:
            assert r.status == "active"

    def test_excludes_completed(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_medications_on_date(bundle, date(2024, 6, 1))
        med_names = [r.medication_display for r in results]
        # Amoxicillin is completed, should not appear
        assert all("amoxicillin" not in n.lower() for n in med_names)

    def test_before_start_date(self):
        """Medications started after query date should not appear."""
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_medications_on_date(bundle, date(2018, 1, 1))
        assert len(results) == 0

    def test_sorted_by_name(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_medications_on_date(bundle, date(2024, 6, 1))
        names = [r.medication_display.lower() for r in results]
        assert names == sorted(names)

    def test_result_fields(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_medications_on_date(bundle, date(2024, 6, 1))
        for r in results:
            assert r.medication_display
            assert r.medication_request_id
            assert r.raw is not None

    def test_empty_bundle(self):
        raw = generate_simple_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_medications_on_date(bundle, date(2024, 6, 1))
        assert len(results) == 0


class TestQueryObservationTrends:
    def test_trend_for_heart_rate(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_observation_trends(bundle, code_filter="heart rate")
        assert len(results) == 1
        trend = results[0]
        assert trend.code_display == "Heart rate"
        assert trend.count == 3
        assert trend.unit == "beats/min"

    def test_trend_values(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_observation_trends(bundle, code_filter="Heart rate")
        trend = results[0]
        # Heart rates: 72, 68, 95
        assert trend.min_value == 68.0
        assert trend.max_value == 95.0
        assert trend.mean_value == pytest.approx(78.33, abs=0.1)

    def test_trend_points_sorted(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_observation_trends(bundle, code_filter="heart rate")
        trend = results[0]
        dates = [p.effective_date for p in trend.points if p.effective_date]
        assert dates == sorted(dates)

    def test_latest_and_earliest(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_observation_trends(bundle, code_filter="heart rate")
        trend = results[0]
        assert trend.latest_value == 95.0
        assert trend.earliest_value == 72.0

    def test_all_trends(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_observation_trends(bundle)
        # Should have trends for: Heart rate, Blood pressure, Body temperature, Body weight, HbA1c
        assert len(results) >= 5

    def test_empty_bundle(self):
        raw = generate_simple_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_observation_trends(bundle)
        assert len(results) == 0

    def test_non_numeric_excluded(self):
        """Observations without numeric values should not appear in trends."""
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_observation_trends(bundle)
        for trend in results:
            assert trend.count > 0
            for point in trend.points:
                assert point.numeric_value is not None

    def test_trend_repr(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_observation_trends(bundle, code_filter="heart rate")
        assert "ObservationTrend" in repr(results[0])

    def test_trend_result_fields(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_observation_trends(bundle, code_filter="heart rate")
        trend = results[0]
        assert trend.code_display
        assert trend.unit == "beats/min"
        for p in trend.points:
            assert p.display_value


class TestIsVitalSign:
    def test_heart_rate_is_vital(self):
        obs = Observation(
            id="o1", resourceType="Observation", status="final",
            code=None,
        )
        from fhir_parser.resources import CodeableConcept, Coding
        obs.code = CodeableConcept(
            coding=[Coding(system="http://loinc.org", code="8867-4", display="Heart rate")]
        )
        assert is_vital_sign(obs) is True

    def test_hba1c_is_not_vital(self):
        obs = Observation(
            id="o1", resourceType="Observation", status="final",
            code=None,
        )
        from fhir_parser.resources import CodeableConcept, Coding
        obs.code = CodeableConcept(
            coding=[Coding(system="http://loinc.org", code="4548-4", display="HbA1c")]
        )
        assert is_vital_sign(obs) is False

    def test_by_category(self):
        obs = Observation(
            id="o1", resourceType="Observation", status="final",
            code=None,
        )
        from fhir_parser.resources import CodeableConcept, Coding
        obs.code = CodeableConcept(
            coding=[Coding(code="99999", display="Unknown")]
        )
        obs.category = [CodeableConcept(
            coding=[Coding(code="vital-signs")]
        )]
        assert is_vital_sign(obs) is True

    def test_no_code(self):
        obs = Observation(
            id="o1", resourceType="Observation", status="final",
        )
        assert is_vital_sign(obs) is False


class TestQueryAllergyIntolerances:
    def test_active_allergies(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_allergy_intolerances(bundle)
        assert len(results) == 2  # Peanut and Penicillin allergies are active

    def test_empty_bundle(self):
        raw = generate_simple_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_allergy_intolerances(bundle)
        assert len(results) == 0


class TestQueryEncounters:
    def test_all_encounters(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_encounters(bundle)
        assert len(results) == 3

    def test_filter_by_status(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_encounters(bundle, status_filter="finished")
        assert len(results) == 3
        for r in results:
            assert r.status == "finished"

    def test_sorted_by_start_date(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_encounters(bundle)
        dates = [r.start_date for r in results if r.start_date]
        assert dates == sorted(dates)


class TestQueryProcedures:
    def test_all_procedures(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_procedures(bundle)
        assert len(results) == 2

    def test_filter_by_status(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        results = query_procedures(bundle, status_filter="completed")
        assert len(results) == 2
        for r in results:
            assert r.status == "completed"
