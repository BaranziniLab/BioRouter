"""
Tests for parse round-trip: parse -> serialize -> parse -> compare.

Ensures that all FHIR resource types survive a round-trip through
the parser and serializer without data loss.
"""

import json
import pytest

from fhir_parser.bundle import BundleFHIR
from fhir_parser.resources import (
    Patient, Encounter, Observation, Condition,
    MedicationRequest, Procedure, AllergyIntolerance,
    parse_resource, serialize_resource,
)
from fhir_parser.synthetic import (
    generate_patient_bundle,
    generate_patient,
    generate_encounter,
    generate_observation,
    generate_condition,
    generate_medication_request,
    generate_procedure,
    generate_allergy_intolerance,
)


# ---------------------------------------------------------------------------
# Individual resource round-trip
# ---------------------------------------------------------------------------

class TestPatientRoundTrip:
    def test_roundtrip(self):
        original = generate_patient()
        p1 = Patient.from_dict(original)
        d1 = p1.to_dict()
        p2 = Patient.from_dict(d1)
        d2 = p2.to_dict()
        assert d1 == d2

    def test_preserves_identifier(self):
        p = Patient.from_dict(generate_patient())
        assert len(p.identifier) > 0
        d = p.to_dict()
        assert len(d["identifier"]) > 0
        assert d["identifier"][0]["value"] == p.identifier[0].value

    def test_preserves_name(self):
        p = Patient.from_dict(generate_patient())
        d = p.to_dict()
        assert d["name"][0]["family"] == "Doe"
        assert d["name"][0]["given"] == ["Jane", "Marie"]

    def test_preserves_address(self):
        p = Patient.from_dict(generate_patient())
        d = p.to_dict()
        assert len(d["address"]) == 1
        assert d["address"][0]["city"] == "San Francisco"

    def test_full_json_string_roundtrip(self):
        original = generate_patient()
        json_str = json.dumps(original)
        p = Patient.from_dict(json.loads(json_str))
        output_str = json.dumps(p.to_dict())
        assert json.loads(json_str) == json.loads(output_str)


class TestEncounterRoundTrip:
    def test_roundtrip(self):
        original = generate_encounter("e1")
        e1 = Encounter.from_dict(original)
        d1 = e1.to_dict()
        e2 = Encounter.from_dict(d1)
        d2 = e2.to_dict()
        assert d1 == d2

    def test_preserves_period(self):
        e = Encounter.from_dict(generate_encounter("e1"))
        d = e.to_dict()
        assert d["period"]["start"] == "2024-01-15T09:00:00Z"

    def test_preserves_class(self):
        e = Encounter.from_dict(generate_encounter("e1"))
        d = e.to_dict()
        assert d["class"]["code"] == "AMB"


class TestObservationRoundTrip:
    def test_roundtrip(self):
        original = generate_observation("o1")
        o1 = Observation.from_dict(original)
        d1 = o1.to_dict()
        o2 = Observation.from_dict(d1)
        d2 = o2.to_dict()
        assert d1 == d2

    def test_preserves_quantity(self):
        o = Observation.from_dict(generate_observation("o1", value=72.0, unit="beats/min"))
        d = o.to_dict()
        assert d["valueQuantity"]["value"] == 72.0
        assert d["valueQuantity"]["unit"] == "beats/min"

    def test_preserves_code(self):
        o = Observation.from_dict(generate_observation("o1", code="8867-4", code_display="Heart rate"))
        d = o.to_dict()
        assert d["code"]["coding"][0]["code"] == "8867-4"


class TestConditionRoundTrip:
    def test_roundtrip(self):
        original = generate_condition("c1")
        c1 = Condition.from_dict(original)
        d1 = c1.to_dict()
        c2 = Condition.from_dict(d1)
        d2 = c2.to_dict()
        assert d1 == d2

    def test_preserves_clinical_status(self):
        c = Condition.from_dict(generate_condition("c1", clinical_status="active"))
        d = c.to_dict()
        assert d["clinicalStatus"]["coding"][0]["code"] == "active"


class TestMedicationRequestRoundTrip:
    def test_roundtrip(self):
        original = generate_medication_request("m1")
        m1 = MedicationRequest.from_dict(original)
        d1 = m1.to_dict()
        m2 = MedicationRequest.from_dict(d1)
        d2 = m2.to_dict()
        assert d1 == d2

    def test_preserves_medication(self):
        m = MedicationRequest.from_dict(generate_medication_request("m1", medication="Aspirin"))
        d = m.to_dict()
        assert d["medicationCodeableConcept"]["text"] == "Aspirin"

    def test_preserves_dosage(self):
        m = MedicationRequest.from_dict(generate_medication_request("m1"))
        d = m.to_dict()
        assert d["dosageInstruction"][0]["text"] == "500 mg oral twice daily"


class TestProcedureRoundTrip:
    def test_roundtrip(self):
        original = generate_procedure("p1")
        p1 = Procedure.from_dict(original)
        d1 = p1.to_dict()
        p2 = Procedure.from_dict(d1)
        d2 = p2.to_dict()
        assert d1 == d2

    def test_preserves_code(self):
        p = Procedure.from_dict(generate_procedure("p1", code_display="CABG"))
        d = p.to_dict()
        assert d["code"]["text"] == "CABG"


class TestAllergyIntoleranceRoundTrip:
    def test_roundtrip(self):
        original = generate_allergy_intolerance("a1")
        a1 = AllergyIntolerance.from_dict(original)
        d1 = a1.to_dict()
        a2 = AllergyIntolerance.from_dict(d1)
        d2 = a2.to_dict()
        assert d1 == d2

    def test_preserves_criticality(self):
        a = AllergyIntolerance.from_dict(generate_allergy_intolerance("a1", criticality="high"))
        d = a.to_dict()
        assert d["criticality"] == "high"


# ---------------------------------------------------------------------------
# Bundle round-trip
# ---------------------------------------------------------------------------

class TestBundleRoundTrip:
    def test_full_bundle_roundtrip(self):
        """Parse -> serialize -> parse -> compare for a full patient bundle."""
        raw = generate_patient_bundle()
        b1 = BundleFHIR.from_dict(raw)
        d1 = b1.to_dict()
        b2 = BundleFHIR.from_dict(d1)
        d2 = b2.to_dict()
        assert d1 == d2

    def test_bundle_resource_count_preserved(self):
        raw = generate_patient_bundle()
        b1 = BundleFHIR.from_dict(raw)
        b2 = BundleFHIR.from_dict(b1.to_dict())
        assert b1.total_resources == b2.total_resources

    def test_bundle_type_preserved(self):
        raw = generate_patient_bundle()
        raw["type"] = "searchset"
        b1 = BundleFHIR.from_dict(raw)
        b2 = BundleFHIR.from_dict(b1.to_dict())
        assert b2.type == "searchset"

    def test_individual_resources_survive_in_bundle(self):
        """Each resource type should survive a round-trip within the bundle."""
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)

        # Round-trip the bundle
        bundle2 = BundleFHIR.from_dict(bundle.to_dict())

        # Check each resource type
        for rtype in ["Patient", "Encounter", "Observation", "Condition",
                       "MedicationRequest", "Procedure", "AllergyIntolerance"]:
            orig = bundle.get_resources_by_type(rtype)
            rt = bundle2.get_resources_by_type(rtype)
            assert len(orig) == len(rt), f"Mismatch in {rtype} count"
            for o, t in zip(orig, rt):
                assert o.to_dict() == t.to_dict(), f"Roundtrip failed for {rtype}/{o.id}"


# ---------------------------------------------------------------------------
# parse_resource round-trip via generic parse_resource/serialize_resource
# ---------------------------------------------------------------------------

class TestGenericParseSerialize:
    @pytest.mark.parametrize("resource_type,gen_func,args", [
        ("Patient", generate_patient, {}),
        ("Encounter", generate_encounter, ("e1",)),
        ("Observation", generate_observation, ("o1",)),
        ("Condition", generate_condition, ("c1",)),
        ("MedicationRequest", generate_medication_request, ("m1",)),
        ("Procedure", generate_procedure, ("p1",)),
        ("AllergyIntolerance", generate_allergy_intolerance, ("a1",)),
    ])
    def test_parse_serialize_roundtrip(self, resource_type, gen_func, args):
        raw = gen_func(*args)
        r1 = parse_resource(raw)
        d1 = serialize_resource(r1)
        r2 = parse_resource(d1)
        d2 = serialize_resource(r2)
        assert d1 == d2
