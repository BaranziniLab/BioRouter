"""
Tests for resources.py — FHIR resource parsing and serialization.
"""

import json
import pytest

from fhir_parser.resources import (
    Patient, Encounter, Observation, Condition,
    MedicationRequest, Procedure, AllergyIntolerance,
    FHIRDateTime, FHIRDate, Reference, CodeableConcept, Coding,
    Quantity, Period, HumanName, ContactPoint, Address, Identifier,
    Meta, Narrative, parse_resource, serialize_resource,
)


# ---------------------------------------------------------------------------
# FHIRDateTime / FHIRDate
# ---------------------------------------------------------------------------

class TestFHIRDateTime:
    def test_full_datetime(self):
        dt = FHIRDateTime("2024-01-15T09:30:00Z")
        assert dt.year == 2024
        assert dt.month == 1
        assert dt.day == 15
        assert str(dt) == "2024-01-15T09:30:00Z"

    def test_date_only(self):
        dt = FHIRDateTime("2024-01-15")
        assert dt.year == 2024
        assert dt.month == 1
        assert dt.day == 15

    def test_none(self):
        dt = FHIRDateTime(None)
        assert dt.raw is None
        assert dt.year is None
        assert str(dt) == ""

    def test_from_value_none(self):
        assert FHIRDateTime.from_value(None) is None

    def test_to_date(self):
        dt = FHIRDateTime("2024-01-15T10:00:00Z")
        d = dt.to_date()
        assert d is not None
        assert d.year == 2024
        assert d.month == 1
        assert d.day == 15

    def test_to_datetime(self):
        dt = FHIRDateTime("2024-01-15T10:30:00")
        d = dt.to_datetime()
        assert d is not None
        assert d.hour == 10
        assert d.minute == 30

    def test_equality(self):
        a = FHIRDateTime("2024-01-15")
        b = FHIRDateTime("2024-01-15")
        c = FHIRDateTime("2024-01-16")
        assert a == b
        assert a != c
        assert a == "2024-01-15"

    def test_hash(self):
        a = FHIRDateTime("2024-01-15")
        b = FHIRDateTime("2024-01-15")
        assert hash(a) == hash(b)
        s = {a, b}
        assert len(s) == 1


class TestFHIRDate:
    def test_basic(self):
        d = FHIRDate("2024-01")
        assert d.raw == "2024-01"
        assert str(d) == "2024-01"

    def test_from_value_none(self):
        assert FHIRDate.from_value(None) is None


# ---------------------------------------------------------------------------
# Reference
# ---------------------------------------------------------------------------

class TestReference:
    def test_parse(self):
        r = Reference.from_dict({"reference": "Patient/123", "display": "Jane Doe"})
        assert r.resource_type == "Patient"
        assert r.resource_id == "123"
        assert r.display == "Jane Doe"

    def test_to_dict(self):
        r = Reference(reference="Patient/123", display="Jane")
        d = r.to_dict()
        assert d["reference"] == "Patient/123"
        assert d["display"] == "Jane"

    def test_none(self):
        assert Reference.from_dict(None) is None

    def test_no_slash(self):
        r = Reference(reference="123")
        assert r.resource_type is None
        assert r.resource_id is None


# ---------------------------------------------------------------------------
# CodeableConcept / Coding
# ---------------------------------------------------------------------------

class TestCodeableConcept:
    def test_parse_with_codings(self):
        cc = CodeableConcept.from_dict({
            "coding": [
                {"system": "http://loinc.org", "code": "8867-4", "display": "Heart rate"}
            ],
            "text": "Heart rate"
        })
        assert cc.text == "Heart rate"
        assert len(cc.coding) == 1
        assert cc.first_code == "8867-4"
        assert cc.first_display == "Heart rate"

    def test_has_code(self):
        cc = CodeableConcept.from_dict({
            "coding": [{"system": "http://loinc.org", "code": "8867-4"}]
        })
        assert cc.has_code("http://loinc.org", "8867-4")
        assert not cc.has_code("http://other.org", "8867-4")

    def test_none(self):
        assert CodeableConcept.from_dict(None) is None

    def test_to_dict_roundtrip(self):
        cc = CodeableConcept.from_dict({
            "coding": [{"system": "x", "code": "y"}],
            "text": "test"
        })
        d = cc.to_dict()
        cc2 = CodeableConcept.from_dict(d)
        assert cc2.text == "test"
        assert cc2.coding[0].code == "y"


# ---------------------------------------------------------------------------
# Quantity / Period
# ---------------------------------------------------------------------------

class TestQuantity:
    def test_basic(self):
        q = Quantity.from_dict({"value": 72.0, "unit": "beats/min", "system": "http://unitsofmeasure.org"})
        assert q.value == 72.0
        assert q.unit == "beats/min"

    def test_to_dict(self):
        q = Quantity(value=5.0, unit="mg")
        d = q.to_dict()
        assert d["value"] == 5.0
        assert d["unit"] == "mg"
        assert "system" not in d

    def test_none(self):
        assert Quantity.from_dict(None) is None


class TestPeriod:
    def test_basic(self):
        p = Period.from_dict({"start": "2024-01-01", "end": "2024-12-31"})
        assert p.start is not None
        assert p.end is not None
        assert str(p.start) == "2024-01-01"

    def test_none(self):
        assert Period.from_dict(None) is None


# ---------------------------------------------------------------------------
# HumanName
# ---------------------------------------------------------------------------

class TestHumanName:
    def test_display_name(self):
        n = HumanName(family="Doe", given=["Jane", "Marie"])
        assert n.display_name == "Jane Marie Doe"

    def test_with_prefix(self):
        n = HumanName(family="Doe", given=["Jane"], prefix=["Ms."])
        assert n.display_name == "Ms. Jane Doe"

    def test_with_text(self):
        n = HumanName(text="Jane Doe", family="Doe", given=["Jane"])
        assert n.display_name == "Jane Doe"

    def test_empty(self):
        n = HumanName()
        assert n.display_name == "Unknown"

    def test_from_dict(self):
        n = HumanName.from_dict({"family": "Smith", "given": ["John"]})
        assert n.display_name == "John Smith"

    def test_to_dict_roundtrip(self):
        n = HumanName(family="Doe", given=["Jane"])
        d = n.to_dict()
        n2 = HumanName.from_dict(d)
        assert n2.family == "Doe"
        assert n2.given == ["Jane"]


# ---------------------------------------------------------------------------
# ContactPoint / Address / Identifier
# ---------------------------------------------------------------------------

class TestContactPoint:
    def test_parse(self):
        cp = ContactPoint.from_dict({"system": "phone", "value": "555-0101", "use": "home"})
        assert cp.system == "phone"
        assert cp.value == "555-0101"


class TestAddress:
    def test_parse(self):
        a = Address.from_dict({
            "line": ["123 Main St"],
            "city": "SF",
            "state": "CA",
            "postalCode": "94105",
        })
        assert a.line == ["123 Main St"]
        assert a.city == "SF"

    def test_to_dict(self):
        a = Address(city="NY", state="NY")
        d = a.to_dict()
        assert d["city"] == "NY"
        assert d["state"] == "NY"


class TestIdentifier:
    def test_parse(self):
        ident = Identifier.from_dict({
            "use": "usual",
            "system": "http://example.org",
            "value": "12345"
        })
        assert ident.value == "12345"


# ---------------------------------------------------------------------------
# Patient
# ---------------------------------------------------------------------------

class TestPatient:
    SAMPLE = {
        "resourceType": "Patient",
        "id": "test-patient-1",
        "identifier": [
            {"use": "usual", "system": "http://example.org/mrn", "value": "MRN-001"}
        ],
        "active": True,
        "name": [
            {"use": "official", "family": "Doe", "given": ["Jane", "Marie"], "prefix": ["Ms."]}
        ],
        "telecom": [
            {"system": "phone", "value": "555-0101", "use": "home"}
        ],
        "gender": "female",
        "birthDate": "1985-03-15",
        "address": [
            {"line": ["123 Main St"], "city": "San Francisco", "state": "CA", "postalCode": "94105"}
        ],
    }

    def test_from_dict(self):
        p = Patient.from_dict(self.SAMPLE)
        assert p.resourceType == "Patient"
        assert p.id == "test-patient-1"
        assert p.gender == "female"
        assert p.display_name == "Ms. Jane Marie Doe"
        assert p.is_deceased is False
        assert len(p.identifier) == 1
        assert p.identifier[0].value == "MRN-001"

    def test_to_dict_roundtrip(self):
        p = Patient.from_dict(self.SAMPLE)
        d = p.to_dict()
        assert d["resourceType"] == "Patient"
        assert d["id"] == "test-patient-1"
        assert d["gender"] == "female"
        assert d["birthDate"] == "1985-03-15"
        assert len(d["name"]) == 1
        assert d["name"][0]["family"] == "Doe"

    def test_full_json_roundtrip(self):
        """Parse -> serialize -> parse -> compare."""
        p1 = Patient.from_dict(self.SAMPLE)
        d1 = p1.to_dict()
        p2 = Patient.from_dict(d1)
        d2 = p2.to_dict()
        assert d1 == d2

    def test_deceased_boolean(self):
        data = dict(self.SAMPLE)
        data["deceasedBoolean"] = True
        p = Patient.from_dict(data)
        assert p.is_deceased is True

    def test_deceased_datetime(self):
        data = dict(self.SAMPLE)
        data["deceasedDateTime"] = "2024-01-01T00:00:00Z"
        p = Patient.from_dict(data)
        assert p.is_deceased is True
        d = p.to_dict()
        assert d["deceasedDateTime"] == "2024-01-01T00:00:00Z"

    def test_repr(self):
        p = Patient.from_dict(self.SAMPLE)
        assert "Patient" in repr(p)
        assert "Doe" in repr(p)

    def test_full_url(self):
        p = Patient.from_dict(self.SAMPLE)
        assert p.full_url == "Patient/test-patient-1"


# ---------------------------------------------------------------------------
# Encounter
# ---------------------------------------------------------------------------

class TestEncounter:
    SAMPLE = {
        "resourceType": "Encounter",
        "id": "enc-1",
        "status": "finished",
        "class": {"system": "http://hl7.org", "code": "AMB", "display": "ambulatory"},
        "type": [{"text": "Office visit"}],
        "subject": {"reference": "Patient/p1"},
        "period": {"start": "2024-01-15T09:00:00Z", "end": "2024-01-15T10:00:00Z"},
    }

    def test_from_dict(self):
        e = Encounter.from_dict(self.SAMPLE)
        assert e.id == "enc-1"
        assert e.status == "finished"
        assert e.class_ is not None
        assert e.subject is not None
        assert e.subject.resource_id == "p1"
        assert e.period is not None

    def test_to_dict_roundtrip(self):
        e1 = Encounter.from_dict(self.SAMPLE)
        d1 = e1.to_dict()
        e2 = Encounter.from_dict(d1)
        d2 = e2.to_dict()
        assert d1 == d2

    def test_start_date(self):
        e = Encounter.from_dict(self.SAMPLE)
        assert e.start_date is not None
        assert e.start_date.year == 2024

    def test_display_class(self):
        e = Encounter.from_dict(self.SAMPLE)
        assert e.display_class == "ambulatory"


# ---------------------------------------------------------------------------
# Observation
# ---------------------------------------------------------------------------

class TestObservation:
    SAMPLE = {
        "resourceType": "Observation",
        "id": "obs-1",
        "status": "final",
        "code": {
            "coding": [{"system": "http://loinc.org", "code": "8867-4", "display": "Heart rate"}],
            "text": "Heart rate"
        },
        "subject": {"reference": "Patient/p1"},
        "effectiveDateTime": "2024-01-15T09:15:00Z",
        "valueQuantity": {"value": 72.0, "unit": "beats/min", "system": "http://unitsofmeasure.org"},
    }

    def test_from_dict(self):
        obs = Observation.from_dict(self.SAMPLE)
        assert obs.id == "obs-1"
        assert obs.status == "final"
        assert obs.code is not None
        assert obs.display_code == "Heart rate"
        assert obs.numeric_value == 72.0
        assert obs.display_value == "72.0 beats/min"

    def test_to_dict_roundtrip(self):
        o1 = Observation.from_dict(self.SAMPLE)
        d1 = o1.to_dict()
        o2 = Observation.from_dict(d1)
        d2 = o2.to_dict()
        assert d1 == d2

    def test_effective_date(self):
        obs = Observation.from_dict(self.SAMPLE)
        assert obs.effective_date is not None

    def test_value_string(self):
        data = dict(self.SAMPLE)
        del data["valueQuantity"]
        data["valueString"] = "Normal"
        obs = Observation.from_dict(data)
        assert obs.display_value == "Normal"
        assert obs.numeric_value is None

    def test_value_boolean(self):
        data = dict(self.SAMPLE)
        del data["valueQuantity"]
        data["valueBoolean"] = True
        obs = Observation.from_dict(data)
        assert obs.display_value == "True"

    def test_components(self):
        data = dict(self.SAMPLE)
        data["component"] = [
            {
                "code": {"coding": [{"code": "8480-6", "display": "Systolic BP"}], "text": "Systolic BP"},
                "valueQuantity": {"value": 120.0, "unit": "mmHg"},
            }
        ]
        obs = Observation.from_dict(data)
        assert len(obs.component) == 1
        assert obs.component[0].numeric_value == 120.0


# ---------------------------------------------------------------------------
# Condition
# ---------------------------------------------------------------------------

class TestCondition:
    SAMPLE = {
        "resourceType": "Condition",
        "id": "cond-1",
        "clinicalStatus": {"coding": [{"code": "active"}]},
        "verificationStatus": {"coding": [{"code": "confirmed"}]},
        "code": {
            "coding": [{"system": "http://snomed.info/sct", "code": "44054006", "display": "Type 2 diabetes"}],
            "text": "Type 2 diabetes"
        },
        "subject": {"reference": "Patient/p1"},
        "onsetDateTime": "2020-06-01",
    }

    def test_from_dict(self):
        c = Condition.from_dict(self.SAMPLE)
        assert c.id == "cond-1"
        assert c.is_active is True
        assert c.display_code == "Type 2 diabetes"
        assert c.onset_date is not None

    def test_to_dict_roundtrip(self):
        c1 = Condition.from_dict(self.SAMPLE)
        d1 = c1.to_dict()
        c2 = Condition.from_dict(d1)
        d2 = c2.to_dict()
        assert d1 == d2

    def test_inactive_condition(self):
        data = dict(self.SAMPLE)
        data["clinicalStatus"] = {"coding": [{"code": "resolved"}]}
        c = Condition.from_dict(data)
        assert c.is_active is False


# ---------------------------------------------------------------------------
# MedicationRequest
# ---------------------------------------------------------------------------

class TestMedicationRequest:
    SAMPLE = {
        "resourceType": "MedicationRequest",
        "id": "med-1",
        "status": "active",
        "intent": "order",
        "medicationCodeableConcept": {
            "coding": [{"system": "http://rxnorm.org", "code": "860975", "display": "Metformin"}],
            "text": "Metformin"
        },
        "subject": {"reference": "Patient/p1"},
        "authoredOn": "2024-01-15T10:00:00Z",
        "dosageInstruction": [
            {
                "text": "500 mg twice daily",
                "doseAndRate": [
                    {"doseQuantity": {"value": 500.0, "unit": "mg"}}
                ]
            }
        ]
    }

    def test_from_dict(self):
        m = MedicationRequest.from_dict(self.SAMPLE)
        assert m.id == "med-1"
        assert m.status == "active"
        assert m.display_medication == "Metformin"
        assert m.is_active is True

    def test_to_dict_roundtrip(self):
        m1 = MedicationRequest.from_dict(self.SAMPLE)
        d1 = m1.to_dict()
        m2 = MedicationRequest.from_dict(d1)
        d2 = m2.to_dict()
        assert d1 == d2

    def test_dosage_text(self):
        m = MedicationRequest.from_dict(self.SAMPLE)
        assert "500 mg" in m.dosage_text

    def test_medication_reference(self):
        data = dict(self.SAMPLE)
        del data["medicationCodeableConcept"]
        data["medicationReference"] = {"reference": "Medication/met-1", "display": "Metformin"}
        m = MedicationRequest.from_dict(data)
        assert m.display_medication == "Metformin"


# ---------------------------------------------------------------------------
# Procedure
# ---------------------------------------------------------------------------

class TestProcedure:
    SAMPLE = {
        "resourceType": "Procedure",
        "id": "proc-1",
        "status": "completed",
        "code": {
            "coding": [{"system": "http://snomed.info/sct", "code": "36969009", "display": "CABG"}],
            "text": "Coronary artery bypass graft"
        },
        "subject": {"reference": "Patient/p1"},
        "performedDateTime": "2023-03-10T08:00:00Z",
    }

    def test_from_dict(self):
        p = Procedure.from_dict(self.SAMPLE)
        assert p.id == "proc-1"
        assert p.status == "completed"
        assert p.display_code == "CABG"
        assert p.performed_date is not None

    def test_to_dict_roundtrip(self):
        p1 = Procedure.from_dict(self.SAMPLE)
        d1 = p1.to_dict()
        p2 = Procedure.from_dict(d1)
        d2 = p2.to_dict()
        assert d1 == d2


# ---------------------------------------------------------------------------
# AllergyIntolerance
# ---------------------------------------------------------------------------

class TestAllergyIntolerance:
    SAMPLE = {
        "resourceType": "AllergyIntolerance",
        "id": "allergy-1",
        "clinicalStatus": {"coding": [{"code": "active"}]},
        "verificationStatus": {"coding": [{"code": "confirmed"}]},
        "criticality": "high",
        "category": ["food"],
        "code": {
            "coding": [{"system": "http://snomed.info/sct", "code": "260147004", "display": "Peanut allergy"}],
            "text": "Peanut allergy"
        },
        "patient": {"reference": "Patient/p1"},
    }

    def test_from_dict(self):
        a = AllergyIntolerance.from_dict(self.SAMPLE)
        assert a.id == "allergy-1"
        assert a.is_active is True
        assert a.display_code == "Peanut allergy"
        assert a.criticality == "high"

    def test_to_dict_roundtrip(self):
        a1 = AllergyIntolerance.from_dict(self.SAMPLE)
        d1 = a1.to_dict()
        a2 = AllergyIntolerance.from_dict(d1)
        d2 = a2.to_dict()
        assert d1 == d2


# ---------------------------------------------------------------------------
# parse_resource / serialize_resource
# ---------------------------------------------------------------------------

class TestParseResource:
    def test_patient(self):
        r = parse_resource({"resourceType": "Patient", "id": "p1"})
        assert isinstance(r, Patient)
        assert r.id == "p1"

    def test_observation(self):
        r = parse_resource({
            "resourceType": "Observation",
            "id": "o1",
            "status": "final",
            "code": {"text": "test"},
        })
        assert isinstance(r, Observation)

    def test_missing_resource_type(self):
        with pytest.raises(ValueError, match="missing"):
            parse_resource({"id": "p1"})

    def test_unsupported_type(self):
        with pytest.raises(ValueError, match="Unsupported"):
            parse_resource({"resourceType": "Binary", "id": "b1"})

    def test_serialize(self):
        p = Patient(id="p1", resourceType="Patient", gender="male")
        d = serialize_resource(p)
        assert d["resourceType"] == "Patient"
        assert d["gender"] == "male"
