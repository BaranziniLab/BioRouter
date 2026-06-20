"""
Tests for validate.py — FHIR validation catches malformed resources.
"""

import pytest

from fhir_parser.bundle import BundleFHIR
from fhir_parser.validate import (
    validate_resource,
    validate_bundle,
    ValidationResult,
    ValidationError,
)
from fhir_parser.resources import (
    Patient, Encounter, Observation, Condition,
    MedicationRequest, Procedure, AllergyIntolerance,
    CodeableConcept, Coding, Reference,
)
from fhir_parser.synthetic import (
    generate_patient_bundle,
    generate_malformed_bundle,
    generate_simple_bundle,
    generate_patient,
    generate_encounter,
    generate_observation,
)


class TestValidationResult:
    def test_empty_is_valid(self):
        r = ValidationResult()
        assert r.is_valid
        assert r.error_count == 0
        assert r.warning_count == 0

    def test_with_errors(self):
        r = ValidationResult()
        r.add(ValidationError("Patient", "p1", "id", "error", "Missing id"))
        assert not r.is_valid
        assert r.error_count == 1

    def test_with_warnings_only(self):
        r = ValidationResult()
        r.add(ValidationError("Patient", "p1", "code", "warning", "Recommended"))
        assert r.is_valid
        assert r.warning_count == 1

    def test_str_valid(self):
        r = ValidationResult()
        assert "passed" in str(r)

    def test_str_invalid(self):
        r = ValidationResult()
        r.add(ValidationError("Patient", "p1", "id", "error", "Missing"))
        assert "failed" in str(r)

    def test_iter(self):
        r = ValidationResult()
        err1 = ValidationError("Patient", "p1", "id", "error", "Missing")
        r.add(err1)
        assert list(r) == [err1]


class TestValidatePatient:
    def test_valid_patient(self):
        p = Patient.from_dict(generate_patient())
        result = validate_resource(p)
        # Should have no errors (maybe a warning)
        assert result.error_count == 0

    def test_missing_id(self):
        p = Patient(id=None, resourceType="Patient", gender="female")
        result = validate_resource(p)
        assert result.error_count >= 1
        messages = [e.message for e in result.errors]
        assert any("id" in m.lower() for m in messages)

    def test_invalid_gender(self):
        p = Patient(id="p1", resourceType="Patient", gender="invalid_gender")
        result = validate_resource(p)
        # Should be a warning (value set)
        warnings = [e for e in result.errors if e.severity == "warning"]
        assert len(warnings) >= 1

    def test_invalid_date_format(self):
        p = Patient(id="p1", resourceType="Patient", gender="male")
        from fhir_parser.resources import FHIRDate
        p.birthDate = FHIRDate("not-a-date")
        result = validate_resource(p)
        errors = [e for e in result.errors if "birthDate" in e.field_path]
        assert len(errors) >= 1


class TestValidateEncounter:
    def test_valid_encounter(self):
        e = Encounter.from_dict(generate_encounter("e1"))
        result = validate_resource(e)
        assert result.error_count == 0

    def test_missing_status(self):
        e = Encounter(id="e1", resourceType="Encounter", status=None)
        result = validate_resource(e)
        assert result.error_count >= 1
        messages = [e.message for e in result.errors]
        assert any("status" in m.lower() for m in messages)

    def test_invalid_status(self):
        e = Encounter(id="e1", resourceType="Encounter", status="INVALID_STATUS")
        result = validate_resource(e)
        assert result.error_count >= 1

    def test_missing_subject(self):
        e = Encounter(id="e1", resourceType="Encounter", status="finished")
        result = validate_resource(e)
        assert result.error_count >= 1
        messages = [e.message for e in result.errors]
        assert any("subject" in m.lower() for m in messages)

    def test_missing_class(self):
        e = Encounter(id="e1", resourceType="Encounter", status="finished", subject=Reference(reference="Patient/p1"))
        result = validate_resource(e)
        assert result.error_count >= 1
        messages = [e.message for e in result.errors]
        assert any("class" in m.lower() for m in messages)


class TestValidateObservation:
    def test_valid_observation(self):
        o = Observation.from_dict(generate_observation("o1"))
        result = validate_resource(o)
        assert result.error_count == 0

    def test_missing_status(self):
        o = Observation(id="o1", resourceType="Observation", status=None)
        result = validate_resource(o)
        assert result.error_count >= 1

    def test_missing_code(self):
        o = Observation(id="o1", resourceType="Observation", status="final", subject=Reference(reference="Patient/p1"))
        result = validate_resource(o)
        assert result.error_count >= 1
        messages = [e.message for e in result.errors]
        assert any("code" in m.lower() for m in messages)

    def test_missing_subject(self):
        o = Observation(
            id="o1", resourceType="Observation", status="final",
            code=CodeableConcept(coding=[Coding(code="8867-4")]),
        )
        result = validate_resource(o)
        assert result.error_count >= 1
        messages = [e.message for e in result.errors]
        assert any("subject" in m.lower() for m in messages)

    def test_no_value_warning(self):
        o = Observation(
            id="o1", resourceType="Observation", status="final",
            code=CodeableConcept(coding=[Coding(code="8867-4")]),
            subject=Reference(reference="Patient/p1"),
        )
        result = validate_resource(o)
        warnings = [e for e in result.errors if e.severity == "warning"]
        assert any("value" in e.message.lower() for e in warnings)

    def test_invalid_status(self):
        o = Observation(
            id="o1", resourceType="Observation", status="INVALID",
            code=CodeableConcept(coding=[Coding(code="8867-4")]),
        )
        result = validate_resource(o)
        assert result.error_count >= 1


class TestValidateCondition:
    def test_valid_condition(self):
        c = Condition.from_dict({
            "resourceType": "Condition",
            "id": "c1",
            "clinicalStatus": {"coding": [{"code": "active"}]},
            "verificationStatus": {"coding": [{"code": "confirmed"}]},
            "code": {"coding": [{"code": "12345", "display": "Test"}]},
            "subject": {"reference": "Patient/p1"},
        })
        result = validate_resource(c)
        assert result.error_count == 0

    def test_invalid_clinical_status(self):
        c = Condition(
            id="c1", resourceType="Condition",
            clinicalStatus=CodeableConcept(coding=[Coding(code="INVALID")]),
            subject=Reference(reference="Patient/p1"),
        )
        result = validate_resource(c)
        assert len(result.errors) >= 1  # may be error or warning

    def test_missing_subject(self):
        c = Condition(
            id="c1", resourceType="Condition",
            clinicalStatus=CodeableConcept(coding=[Coding(code="active")]),
        )
        result = validate_resource(c)
        assert result.error_count >= 1


class TestValidateMedicationRequest:
    def test_valid(self):
        m = MedicationRequest.from_dict({
            "resourceType": "MedicationRequest",
            "id": "m1",
            "status": "active",
            "intent": "order",
            "medicationCodeableConcept": {"text": "Aspirin"},
            "subject": {"reference": "Patient/p1"},
        })
        result = validate_resource(m)
        assert result.error_count == 0

    def test_missing_status(self):
        m = MedicationRequest(id="m1", resourceType="MedicationRequest", status=None, intent="order")
        result = validate_resource(m)
        assert result.error_count >= 1

    def test_invalid_status(self):
        m = MedicationRequest(id="m1", resourceType="MedicationRequest", status="INVALID", intent="order")
        result = validate_resource(m)
        assert result.error_count >= 1

    def test_missing_intent(self):
        m = MedicationRequest(id="m1", resourceType="MedicationRequest", status="active", intent=None)
        result = validate_resource(m)
        assert result.error_count >= 1

    def test_missing_medication(self):
        m = MedicationRequest(
            id="m1", resourceType="MedicationRequest",
            status="active", intent="order",
            subject=Reference(reference="Patient/p1"),
        )
        result = validate_resource(m)
        assert result.error_count >= 1

    def test_invalid_intent(self):
        m = MedicationRequest(id="m1", resourceType="MedicationRequest", status="active", intent="INVALID")
        result = validate_resource(m)
        assert result.error_count >= 1


class TestValidateProcedure:
    def test_valid(self):
        p = Procedure.from_dict({
            "resourceType": "Procedure",
            "id": "p1",
            "status": "completed",
            "subject": {"reference": "Patient/p1"},
        })
        result = validate_resource(p)
        assert result.error_count == 0

    def test_missing_status(self):
        p = Procedure(id="p1", resourceType="Procedure", status=None)
        result = validate_resource(p)
        assert result.error_count >= 1

    def test_invalid_status(self):
        p = Procedure(id="p1", resourceType="Procedure", status="INVALID")
        result = validate_resource(p)
        assert result.error_count >= 1

    def test_missing_subject(self):
        p = Procedure(id="p1", resourceType="Procedure", status="completed")
        result = validate_resource(p)
        assert result.error_count >= 1


class TestValidateAllergyIntolerance:
    def test_valid(self):
        a = AllergyIntolerance.from_dict({
            "resourceType": "AllergyIntolerance",
            "id": "a1",
            "clinicalStatus": {"coding": [{"code": "active"}]},
            "criticality": "high",
            "patient": {"reference": "Patient/p1"},
        })
        result = validate_resource(a)
        assert result.error_count == 0

    def test_invalid_criticality(self):
        a = AllergyIntolerance(
            id="a1", resourceType="AllergyIntolerance",
            criticality="INVALID",
            patient=Reference(reference="Patient/p1"),
        )
        result = validate_resource(a)
        assert len(result.errors) >= 1  # may be error or warning

    def test_invalid_clinical_status(self):
        a = AllergyIntolerance(
            id="a1", resourceType="AllergyIntolerance",
            clinicalStatus=CodeableConcept(coding=[Coding(code="INVALID")]),
            criticality="high",
            patient=Reference(reference="Patient/p1"),
        )
        result = validate_resource(a)
        assert len(result.errors) >= 1  # may be error or warning


class TestValidateBundle:
    def test_valid_bundle(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        result = validate_bundle(bundle)
        # Our synthetic data should be fully valid
        assert result.is_valid, f"Unexpected errors: {result.errors}"

    def test_malformed_bundle(self):
        raw = generate_malformed_bundle()
        bundle = BundleFHIR.from_dict(raw)
        result = validate_bundle(bundle)
        assert not result.is_valid
        assert result.error_count >= 5  # Multiple issues

    def test_malformed_patient_missing_name(self):
        """Missing patient name should be caught."""
        raw = generate_malformed_bundle()
        bundle = BundleFHIR.from_dict(raw)
        result = validate_bundle(bundle)
        messages = [e.message for e in result.errors]
        assert any("name" in m.lower() for m in messages)

    def test_malformed_encounter_missing_fields(self):
        """Missing encounter status, class, subject should be caught."""
        raw = generate_malformed_bundle()
        bundle = BundleFHIR.from_dict(raw)
        result = validate_bundle(bundle)
        messages = [e.message for e in result.errors]
        # Should have errors about status, class, subject
        assert any("status" in m.lower() for m in messages)

    def test_malformed_observation_missing_fields(self):
        """Missing observation status, code, subject should be caught."""
        raw = generate_malformed_bundle()
        bundle = BundleFHIR.from_dict(raw)
        result = validate_bundle(bundle)
        messages = [e.message for e in result.errors]
        assert any("code" in m.lower() for m in messages)

    def test_malformed_condition_invalid_status(self):
        """Invalid condition clinical status should be caught."""
        raw = generate_malformed_bundle()
        bundle = BundleFHIR.from_dict(raw)
        result = validate_bundle(bundle)
        messages = [e.message for e in result.errors]
        assert any("clinicalStatus" in m or "INVALID_STATUS" in m for m in messages)

    def test_malformed_medication_invalid_fields(self):
        """Invalid medication request status/intent should be caught."""
        raw = generate_malformed_bundle()
        bundle = BundleFHIR.from_dict(raw)
        result = validate_bundle(bundle)
        messages = [e.message for e in result.errors]
        assert any("status" in m.lower() or "intent" in m.lower() for m in messages)

    def test_reference_integrity(self):
        """Unresolvable references should be caught."""
        raw = generate_malformed_bundle()
        bundle = BundleFHIR.from_dict(raw)
        result = validate_bundle(bundle)
        messages = [e.message for e in result.errors]
        assert any("reference" in m.lower() and "cannot be resolved" in m.lower() for m in messages)

    def test_invalid_id_format(self):
        """Invalid id format should be caught."""
        raw = generate_malformed_bundle()
        bundle = BundleFHIR.from_dict(raw)
        result = validate_bundle(bundle)
        messages = [e.message for e in result.errors]
        assert any("id" in m.lower() for m in messages)


class TestValidationError:
    def test_str(self):
        e = ValidationError("Patient", "p1", "name", "error", "Missing name")
        s = str(e)
        assert "Patient" in s
        assert "p1" in s
        assert "name" in s
        assert "ERROR" in s

    def test_repr(self):
        e = ValidationError("Patient", "p1", "name", "error", "Missing name")
        assert "Patient" in repr(e)
        assert "name" in repr(e)
