"""
Tests for bundle.py — Bundle parsing, reference resolution, type extraction.
"""

import json
import pytest

from fhir_parser.bundle import BundleFHIR, BundleEntry, parse_bundle, merge_bundles
from fhir_parser.resources import Patient, Observation, Condition
from fhir_parser.synthetic import (
    generate_patient_bundle,
    generate_simple_bundle,
    generate_malformed_bundle,
    generate_empty_bundle,
)


class TestBundleFHIR:
    def test_from_dict(self):
        raw = generate_simple_bundle()
        bundle = BundleFHIR.from_dict(raw)
        assert bundle.type == "collection"
        assert len(bundle.entry) == 2
        assert bundle.total_resources == 2

    def test_from_json(self):
        raw = generate_simple_bundle()
        json_str = json.dumps(raw)
        bundle = BundleFHIR.from_json(json_str)
        assert bundle.type == "collection"
        assert len(bundle.entry) == 2

    def test_from_json_list(self):
        resources = [
            {"resourceType": "Patient", "id": "p1"},
            {"resourceType": "Observation", "id": "o1", "status": "final", "code": {"text": "test"}},
        ]
        bundle = BundleFHIR.from_json(json.dumps(resources))
        assert bundle.type == "collection"
        assert len(bundle.entry) == 2

    def test_get_patient(self):
        raw = generate_simple_bundle()
        bundle = BundleFHIR.from_dict(raw)
        patient = bundle.get_patient()
        assert patient is not None
        assert patient.id == "simple-patient"

    def test_get_patient_none(self):
        raw = {
            "resourceType": "Bundle",
            "type": "collection",
            "entry": [{"resource": {"resourceType": "Observation", "id": "o1", "status": "final", "code": {"text": "x"}}}],
        }
        bundle = BundleFHIR.from_dict(raw)
        assert bundle.get_patient() is None

    def test_get_resources_by_type(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        conditions = bundle.get_resources_by_type("Condition")
        assert len(conditions) == 4
        assert all(isinstance(c, Condition) for c in conditions)

    def test_get_entries_by_type(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        obs_entries = bundle.get_entries_by_type("Observation")
        assert len(obs_entries) == 12

    def test_resource_type_counts(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        counts = bundle.resource_type_counts
        assert "Patient" in counts
        assert counts["Patient"] == 1
        assert "Encounter" in counts
        assert counts["Encounter"] == 3

    def test_patient_count(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        assert bundle.patient_count == 1

    def test_to_dict(self):
        raw = generate_simple_bundle()
        bundle = BundleFHIR.from_dict(raw)
        d = bundle.to_dict()
        assert d["resourceType"] == "Bundle"
        assert len(d["entry"]) == 2

    def test_to_json_roundtrip(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        json_str = bundle.to_json()
        bundle2 = BundleFHIR.from_json(json_str)
        assert bundle2.total_resources == bundle.total_resources

    def test_from_resource_list(self):
        p = Patient(id="p1", resourceType="Patient", gender="female")
        obs = Observation(id="o1", resourceType="Observation", status="final")
        bundle = BundleFHIR.from_resource_list([p, obs])
        assert len(bundle.entry) == 2
        assert bundle.total_resources == 2

    def test_iter(self):
        raw = generate_simple_bundle()
        bundle = BundleFHIR.from_dict(raw)
        entries = list(bundle)
        assert len(entries) == 2

    def test_len(self):
        raw = generate_simple_bundle()
        bundle = BundleFHIR.from_dict(raw)
        assert len(bundle) == 2

    def test_repr(self):
        raw = generate_simple_bundle()
        bundle = BundleFHIR.from_dict(raw)
        assert "BundleFHIR" in repr(bundle)

    def test_empty_bundle(self):
        raw = generate_empty_bundle()
        bundle = BundleFHIR.from_dict(raw)
        assert len(bundle.entry) == 0
        assert bundle.total_resources == 0
        assert bundle.get_patient() is None

    def test_unknown_resource_type_skipped(self):
        raw = {
            "resourceType": "Bundle",
            "type": "collection",
            "entry": [
                {"resource": {"resourceType": "Patient", "id": "p1"}},
                {"resource": {"resourceType": "Binary", "id": "b1", "data": "abc"}},
            ],
        }
        bundle = BundleFHIR.from_dict(raw)
        assert len(bundle.entry) == 2
        assert bundle.total_resources == 1


class TestReferenceResolution:
    def test_resolve_by_type_id(self):
        raw = generate_simple_bundle()
        bundle = BundleFHIR.from_dict(raw)
        resolved = bundle.resolve_reference("Patient/simple-patient")
        assert resolved is not None
        assert isinstance(resolved, Patient)

    def test_resolve_nonexistent(self):
        raw = generate_simple_bundle()
        bundle = BundleFHIR.from_dict(raw)
        assert bundle.resolve_reference("Patient/nonexistent") is None

    def test_resolve_empty_string(self):
        raw = generate_simple_bundle()
        bundle = BundleFHIR.from_dict(raw)
        assert bundle.resolve_reference("") is None

    def test_observation_resolves_subject(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        obs = bundle.get_resources_by_type("Observation")[0]
        assert isinstance(obs, Observation)
        if obs.subject and obs.subject.reference:
            resolved = bundle.resolve_reference(obs.subject.reference)
            assert resolved is not None
            assert isinstance(resolved, Patient)

    def test_full_patient_bundle_resolution(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        for entry in bundle:
            if entry.resource is None:
                continue
            for field_name in ("subject", "patient", "encounter", "recorder"):
                ref_obj = getattr(entry.resource, field_name, None)
                if ref_obj and hasattr(ref_obj, "reference") and ref_obj.reference:
                    ref_str = ref_obj.reference
                    if ref_str.startswith("Patient/"):
                        resolved = bundle.resolve_reference(ref_str)
                        assert resolved is not None, f"Failed to resolve {ref_str}"


class TestParseBundle:
    def test_with_string(self):
        raw = generate_simple_bundle()
        bundle = parse_bundle(json.dumps(raw))
        assert isinstance(bundle, BundleFHIR)

    def test_with_dict(self):
        raw = generate_simple_bundle()
        bundle = parse_bundle(raw)
        assert isinstance(bundle, BundleFHIR)


class TestMergeBundles:
    def test_merge_two_bundles(self):
        raw1 = generate_simple_bundle()
        raw2 = generate_simple_bundle()
        raw2["entry"][0]["resource"]["id"] = "simple-patient-2"
        raw2["entry"][0]["resource"]["name"] = [{"family": "Smith", "given": ["John"]}]
        raw2["entry"][1]["resource"]["id"] = "simple-enc-2"
        raw2["entry"][1]["resource"]["subject"] = {"reference": "Patient/simple-patient-2"}

        b1 = BundleFHIR.from_dict(raw1)
        b2 = BundleFHIR.from_dict(raw2)
        merged = merge_bundles(b1, b2)
        assert merged.total_resources == 4

    def test_merge_deduplicates(self):
        raw = generate_simple_bundle()
        b1 = BundleFHIR.from_dict(raw)
        b2 = BundleFHIR.from_dict(raw)
        merged = merge_bundles(b1, b2)
        assert merged.total_resources == 2


class TestBundleEntry:
    def test_resource_type(self):
        e = BundleEntry(fullUrl="Patient/p1", resource=Patient(id="p1", resourceType="Patient"))
        assert e.resource_type == "Patient"
        assert e.resource_id == "p1"

    def test_from_dict_with_resource(self):
        e = BundleEntry.from_dict({
            "fullUrl": "Patient/p1",
            "resource": {"resourceType": "Patient", "id": "p1"},
        })
        assert e.resource is not None
        assert isinstance(e.resource, Patient)

    def test_from_dict_without_resource(self):
        e = BundleEntry.from_dict({"fullUrl": "Patient/p1"})
        assert e.resource is None

    def test_to_dict(self):
        e = BundleEntry(fullUrl="Patient/p1", resource=Patient(id="p1", resourceType="Patient"))
        d = e.to_dict()
        assert d["fullUrl"] == "Patient/p1"
        assert d["resource"]["resourceType"] == "Patient"

    def test_repr(self):
        e = BundleEntry(fullUrl="Patient/p1", resource=Patient(id="p1", resourceType="Patient"))
        assert "Patient" in repr(e)
