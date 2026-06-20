"""
Synthetic FHIR Bundle Generator.

Creates realistic FHIR R4 bundles for testing the parser, timeline,
query engine, and validator. All data is entirely synthetic.
"""

from __future__ import annotations

import json
from datetime import datetime, timedelta
from typing import Any

from .bundle import BundleFHIR
from .resources import FHIRResource


def _random_id(seed: int = 0) -> str:
    """Simple deterministic ID generator for reproducibility."""
    return f"syn-{seed:04d}"


# ---------------------------------------------------------------------------
# Individual resource generators
# ---------------------------------------------------------------------------

def generate_patient(patient_id: str = "patient-1", **overrides: Any) -> dict:
    """Generate a synthetic Patient resource dict."""
    d: dict[str, Any] = {
        "resourceType": "Patient",
        "id": patient_id,
        "identifier": [
            {
                "use": "usual",
                "system": "http://example.org/fhir/mrn",
                "value": f"MRN-{patient_id}",
            }
        ],
        "active": True,
        "name": [
            {
                "use": "official",
                "family": "Doe",
                "given": ["Jane", "Marie"],
                "prefix": ["Ms."],
            }
        ],
        "telecom": [
            {"system": "phone", "value": "555-0101", "use": "home"},
            {"system": "email", "value": "jane.doe@example.com", "use": "home"},
        ],
        "gender": "female",
        "birthDate": "1985-03-15",
        "address": [
            {
                "use": "home",
                "line": ["123 Main St"],
                "city": "San Francisco",
                "state": "CA",
                "postalCode": "94105",
                "country": "US",
            }
        ],
        "maritalStatus": {
            "coding": [
                {"system": "http://terminology.hl7.org/CodeSystem/v3-MaritalStatus", "code": "M", "display": "Married"}
            ],
            "text": "Married",
        },
    }
    d.update(overrides)
    return d


def generate_encounter(
    encounter_id: str,
    patient_id: str = "patient-1",
    start: str = "2024-01-15T09:00:00Z",
    end: str = "2024-01-15T10:30:00Z",
    status: str = "finished",
    enc_class: str = "AMB",
    encounter_type: str = "Office visit",
    **overrides: Any,
) -> dict:
    """Generate a synthetic Encounter resource dict."""
    d: dict[str, Any] = {
        "resourceType": "Encounter",
        "id": encounter_id,
        "status": status,
        "class": {"system": "http://terminology.hl7.org/CodeSystem/v3-ActCode", "code": enc_class, "display": enc_class},
        "type": [
            {
                "coding": [
                    {"system": "http://snomed.info/sct", "code": "185349003", "display": encounter_type}
                ],
                "text": encounter_type,
            }
        ],
        "subject": {"reference": f"Patient/{patient_id}", "display": "Jane Doe"},
        "period": {"start": start, "end": end},
        "reasonCode": [
            {
                "coding": [
                    {"system": "http://snomed.info/sct", "code": "386661006", "display": "Fever"}
                ],
                "text": "Fever",
            }
        ],
    }
    d.update(overrides)
    return d


def generate_observation(
    obs_id: str,
    patient_id: str = "patient-1",
    encounter_id: str | None = "encounter-1",
    code: str = "8867-4",
    code_display: str = "Heart rate",
    code_system: str = "http://loinc.org",
    value: float = 72.0,
    unit: str = "beats/min",
    effective: str = "2024-01-15T09:15:00Z",
    status: str = "final",
    category_code: str = "vital-signs",
    **overrides: Any,
) -> dict:
    """Generate a synthetic Observation resource dict."""
    d: dict[str, Any] = {
        "resourceType": "Observation",
        "id": obs_id,
        "status": status,
        "category": [
            {
                "coding": [
                    {"system": "http://terminology.hl7.org/CodeSystem/observation-category", "code": category_code, "display": category_code}
                ]
            }
        ],
        "code": {
            "coding": [
                {"system": code_system, "code": code, "display": code_display}
            ],
            "text": code_display,
        },
        "subject": {"reference": f"Patient/{patient_id}"},
        "effectiveDateTime": effective,
        "valueQuantity": {
            "value": value,
            "unit": unit,
            "system": "http://unitsofmeasure.org",
            "code": unit,
        },
    }
    if encounter_id:
        d["encounter"] = {"reference": f"Encounter/{encounter_id}"}
    d.update(overrides)
    return d


def generate_condition(
    condition_id: str,
    patient_id: str = "patient-1",
    code: str = "44054006",
    code_display: str = "Type 2 diabetes mellitus",
    code_system: str = "http://snomed.info/sct",
    clinical_status: str = "active",
    verification_status: str = "confirmed",
    onset: str = "2020-06-01",
    **overrides: Any,
) -> dict:
    """Generate a synthetic Condition resource dict."""
    d: dict[str, Any] = {
        "resourceType": "Condition",
        "id": condition_id,
        "clinicalStatus": {
            "coding": [
                {"system": "http://terminology.hl7.org/CodeSystem/condition-clinical", "code": clinical_status}
            ]
        },
        "verificationStatus": {
            "coding": [
                {"system": "http://terminology.hl7.org/CodeSystem/condition-ver-status", "code": verification_status}
            ]
        },
        "category": [
            {
                "coding": [
                    {"system": "http://terminology.hl7.org/CodeSystem/condition-category", "code": "encounter-diagnosis", "display": "Encounter Diagnosis"}
                ]
            }
        ],
        "code": {
            "coding": [
                {"system": code_system, "code": code, "display": code_display}
            ],
            "text": code_display,
        },
        "subject": {"reference": f"Patient/{patient_id}"},
        "onsetDateTime": onset,
    }
    d.update(overrides)
    return d


def generate_medication_request(
    med_id: str,
    patient_id: str = "patient-1",
    medication: str = "Metformin",
    medication_code: str = "860975",
    status: str = "active",
    authored: str = "2023-06-01T10:00:00Z",
    dosage_text: str = "500 mg oral twice daily",
    dose_value: float = 500.0,
    dose_unit: str = "mg",
    **overrides: Any,
) -> dict:
    """Generate a synthetic MedicationRequest resource dict."""
    d: dict[str, Any] = {
        "resourceType": "MedicationRequest",
        "id": med_id,
        "status": status,
        "intent": "order",
        "medicationCodeableConcept": {
            "coding": [
                {"system": "http://www.nlm.nih.gov/research/umls/rxnorm", "code": medication_code, "display": medication}
            ],
            "text": medication,
        },
        "subject": {"reference": f"Patient/{patient_id}"},
        "authoredOn": authored,
        "dosageInstruction": [
            {
                "text": dosage_text,
                "doseAndRate": [
                    {
                        "doseQuantity": {
                            "value": dose_value,
                            "unit": dose_unit,
                            "system": "http://unitsofmeasure.org",
                            "code": dose_unit,
                        }
                    }
                ],
            }
        ],
    }
    d.update(overrides)
    return d


def generate_procedure(
    proc_id: str,
    patient_id: str = "patient-1",
    code: str = "36969009",
    code_display: str = "Coronary artery bypass graft",
    status: str = "completed",
    performed: str = "2023-03-10T08:00:00Z",
    **overrides: Any,
) -> dict:
    """Generate a synthetic Procedure resource dict."""
    d: dict[str, Any] = {
        "resourceType": "Procedure",
        "id": proc_id,
        "status": status,
        "code": {
            "coding": [
                {"system": "http://snomed.info/sct", "code": code, "display": code_display}
            ],
            "text": code_display,
        },
        "subject": {"reference": f"Patient/{patient_id}"},
        "performedDateTime": performed,
    }
    d.update(overrides)
    return d


def generate_allergy_intolerance(
    allergy_id: str,
    patient_id: str = "patient-1",
    code: str = "260147004",
    code_display: str = "Peanut allergy",
    clinical_status: str = "active",
    verification_status: str = "confirmed",
    criticality: str = "high",
    category: str = "food",
    **overrides: Any,
) -> dict:
    """Generate a synthetic AllergyIntolerance resource dict."""
    d: dict[str, Any] = {
        "resourceType": "AllergyIntolerance",
        "id": allergy_id,
        "clinicalStatus": {
            "coding": [
                {"system": "http://terminology.hl7.org/CodeSystem/allergyintolerance-clinical", "code": clinical_status}
            ]
        },
        "verificationStatus": {
            "coding": [
                {"system": "http://terminology.hl7.org/CodeSystem/allergyintolerance-verification", "code": verification_status}
            ]
        },
        "type": {
            "coding": [
                {"system": "http://terminology.hl7.org/CodeSystem/allergyintolerance-type", "code": "allergy"}
            ]
        },
        "category": [category],
        "criticality": criticality,
        "code": {
            "coding": [
                {"system": "http://snomed.info/sct", "code": code, "display": code_display}
            ],
            "text": code_display,
        },
        "patient": {"reference": f"Patient/{patient_id}"},
        "recordedDate": "2022-01-20",
    }
    d.update(overrides)
    return d


# ---------------------------------------------------------------------------
# Full bundle generators
# ---------------------------------------------------------------------------

def generate_patient_bundle(patient_id: str = "patient-1") -> dict:
    """Generate a complete patient bundle with all resource types.

    Returns a dict that can be passed to parse_bundle().
    """
    encounter_base = datetime(2024, 1, 15, 9, 0, 0)
    obs_base = datetime(2024, 1, 15, 9, 15, 0)

    entries = []

    # Patient
    entries.append({
        "fullUrl": f"urn:uuid:{patient_id}",
        "resource": generate_patient(patient_id),
        "search": {"mode": "match"},
    })

    # Encounters
    encounters = [
        ("encounter-1", "2024-01-15T09:00:00Z", "2024-01-15T10:30:00Z", "finished", "AMB", "Office visit"),
        ("encounter-2", "2024-03-20T14:00:00Z", "2024-03-20T15:00:00Z", "finished", "AMB", "Follow-up"),
        ("encounter-3", "2024-06-10T11:00:00Z", "2024-06-10T12:00:00Z", "finished", "EMER", "Emergency visit"),
    ]
    for eid, start, end, status, enc_class, enc_type in encounters:
        entries.append({
            "fullUrl": f"urn:uuid:{eid}",
            "resource": generate_encounter(
                eid, patient_id, start, end, status, enc_class, enc_type
            ),
        })

    # Observations — vitals across encounters
    obs_data = [
        ("obs-hr-1", "encounter-1", "8867-4", "Heart rate", 72.0, "beats/min", "2024-01-15T09:15:00Z"),
        ("obs-bp-1", "encounter-1", "85354-9", "Blood pressure", 120.0, "mmHg", "2024-01-15T09:15:00Z"),
        ("obs-temp-1", "encounter-1", "8310-5", "Body temperature", 101.2, "F", "2024-01-15T09:15:00Z"),
        ("obs-wt-1", "encounter-1", "29463-7", "Body weight", 165.0, "[lb_av]", "2024-01-15T09:20:00Z"),
        ("obs-hr-2", "encounter-2", "8867-4", "Heart rate", 68.0, "beats/min", "2024-03-20T14:15:00Z"),
        ("obs-bp-2", "encounter-2", "85354-9", "Blood pressure", 118.0, "mmHg", "2024-03-20T14:15:00Z"),
        ("obs-wt-2", "encounter-2", "29463-7", "Body weight", 162.0, "[lb_av]", "2024-03-20T14:20:00Z"),
        ("obs-hr-3", "encounter-3", "8867-4", "Heart rate", 95.0, "beats/min", "2024-06-10T11:10:00Z"),
        ("obs-bp-3", "encounter-3", "85354-9", "Blood pressure", 140.0, "mmHg", "2024-06-10T11:10:00Z"),
        ("obs-temp-3", "encounter-3", "8310-5", "Body temperature", 102.5, "F", "2024-06-10T11:10:00Z"),
        # Non-vital lab observation
        ("obs-a1c-1", None, "4548-4", "HbA1c", 7.2, "%", "2024-01-15T09:30:00Z"),
        ("obs-a1c-2", None, "4548-4", "HbA1c", 6.8, "%", "2024-06-10T11:30:00Z"),
    ]
    for oid, eid, code, display, val, unit, eff in obs_data:
        entries.append({
            "fullUrl": f"urn:uuid:{oid}",
            "resource": generate_observation(oid, patient_id, eid, code, display, value=val, unit=unit, effective=eff),
        })

    # Conditions
    conditions = [
        ("cond-1", "44054006", "Type 2 diabetes mellitus", "active", "confirmed", "2020-06-01"),
        ("cond-2", "38341003", "Hypertensive disorder", "active", "confirmed", "2019-01-15"),
        ("cond-3", "195967002", "Hyperlipidemia", "active", "confirmed", "2021-03-10"),
        ("cond-4", "275495004", "Pneumonia", "resolved", "confirmed", "2024-01-20"),
    ]
    for cid, code, display, cs, vs, onset in conditions:
        entries.append({
            "fullUrl": f"urn:uuid:{cid}",
            "resource": generate_condition(
                cid, patient_id, code, display,
                clinical_status=cs, verification_status=vs, onset=onset,
            ),
        })

    # Medications
    medications = [
        ("med-1", "Metformin", "860975", "active", "2020-06-01T10:00:00Z", "500 mg oral twice daily", 500.0, "mg"),
        ("med-2", "Lisinopril", "314076", "active", "2019-01-15T10:00:00Z", "10 mg oral once daily", 10.0, "mg"),
        ("med-3", "Atorvastatin", "83367", "active", "2021-03-10T10:00:00Z", "20 mg oral once daily", 20.0, "mg"),
        ("med-4", "Amoxicillin", "726002", "completed", "2024-01-20T10:00:00Z", "500 mg oral three times daily", 500.0, "mg"),
    ]
    for mid, med, code, status, auth, dosage, dv, du in medications:
        entries.append({
            "fullUrl": f"urn:uuid:{mid}",
            "resource": generate_medication_request(mid, patient_id, med, code, status, auth, dosage, dv, du),
        })

    # Procedures
    procedures = [
        ("proc-1", "36969009", "Coronary artery bypass graft", "completed", "2023-03-10T08:00:00Z"),
        ("proc-2", "17112001", "Lumbar puncture", "completed", "2024-06-10T11:45:00Z"),
    ]
    for pid, code, display, status, performed in procedures:
        entries.append({
            "fullUrl": f"urn:uuid:{pid}",
            "resource": generate_procedure(pid, patient_id, code, display, status, performed),
        })

    # Allergies
    allergies = [
        ("allergy-1", "260147004", "Peanut allergy", "active", "confirmed", "high", "food"),
        ("allergy-2", "7980", "Penicillin allergy", "active", "confirmed", "high", "medication"),
    ]
    for aid, code, display, cs, vs, crit, cat in allergies:
        entries.append({
            "fullUrl": f"urn:uuid:{aid}",
            "resource": generate_allergy_intolerance(aid, patient_id, code, display, cs, vs, crit, cat),
        })

    return {
        "resourceType": "Bundle",
        "type": "collection",
        "total": len(entries),
        "entry": entries,
    }


def generate_malformed_bundle() -> dict:
    """Generate a bundle with deliberately malformed resources for validation testing."""
    return {
        "resourceType": "Bundle",
        "type": "collection",
        "entry": [
            # Patient with missing required fields
            {
                "resource": {
                    "resourceType": "Patient",
                    "id": "bad-patient-1",
                    # missing: identifier, name
                    "gender": "invalid_gender",
                    "birthDate": "not-a-date",
                },
            },
            # Encounter with missing required fields
            {
                "resource": {
                    "resourceType": "Encounter",
                    "id": "bad-enc-1",
                    # missing: status, class, subject
                },
            },
            # Observation with missing required fields
            {
                "resource": {
                    "resourceType": "Observation",
                    "id": "bad-obs-1",
                    # missing: status, code, subject
                },
            },
            # Condition with invalid status codes
            {
                "resource": {
                    "resourceType": "Condition",
                    "id": "bad-cond-1",
                    "clinicalStatus": {
                        "coding": [{"code": "INVALID_STATUS"}]
                    },
                    "subject": {"reference": "Patient/bad-patient-1"},
                },
            },
            # MedicationRequest with invalid status
            {
                "resource": {
                    "resourceType": "MedicationRequest",
                    "id": "bad-med-1",
                    "status": "INVALID_STATUS",
                    "intent": "INVALID_INTENT",
                    "subject": {"reference": "Patient/bad-patient-1"},
                },
            },
            # Observation with broken reference
            {
                "resource": {
                    "resourceType": "Observation",
                    "id": "obs-bad-ref",
                    "status": "final",
                    "code": {
                        "coding": [{"system": "http://loinc.org", "code": "8867-4", "display": "Heart rate"}],
                        "text": "Heart rate",
                    },
                    "subject": {"reference": "Patient/nonexistent-patient"},
                    "effectiveDateTime": "2024-01-15T09:00:00Z",
                    "valueQuantity": {"value": 72.0, "unit": "beats/min"},
                },
            },
            # Patient with invalid id format
            {
                "resource": {
                    "resourceType": "Patient",
                    "id": "bad id with spaces!@#$",
                    "name": [{"family": "Test"}],
                    "gender": "male",
                },
            },
        ],
    }


def generate_empty_bundle() -> dict:
    """Generate an empty bundle."""
    return {
        "resourceType": "Bundle",
        "type": "collection",
        "total": 0,
        "entry": [],
    }


def generate_simple_bundle() -> dict:
    """Generate a minimal bundle with just a patient and one encounter."""
    return {
        "resourceType": "Bundle",
        "type": "collection",
        "entry": [
            {
                "resource": generate_patient("simple-patient"),
            },
            {
                "resource": generate_encounter(
                    "simple-enc-1",
                    patient_id="simple-patient",
                    start="2024-06-01T10:00:00Z",
                    end="2024-06-01T11:00:00Z",
                ),
            },
        ],
    }


def generate_multi_patient_bundle() -> dict:
    """Generate a bundle with multiple patients for cross-patient queries."""
    base = generate_patient_bundle("patient-1")

    # Add a second patient
    p2_entries = [
        {"resource": generate_patient("patient-2", name=[{"use": "official", "family": "Smith", "given": ["John"]}], gender="male", birthDate="1970-08-22")},
        {"resource": generate_encounter("enc-p2-1", "patient-2", "2024-02-10T08:00:00Z", "2024-02-10T09:00:00Z")},
        {"resource": generate_observation("obs-p2-hr-1", "patient-2", "enc-p2-1", "8867-4", "Heart rate", 78.0, "beats/min", "2024-02-10T08:15:00Z")},
        {"resource": generate_condition("cond-p2-1", "patient-2", "195967002", "Hyperlipidemia", "active", "confirmed", "2022-05-01")},
    ]

    base["entry"].extend(p2_entries)
    base["total"] = len(base["entry"])
    return base


# ---------------------------------------------------------------------------
# Convenience: dict -> BundleFHIR
# ---------------------------------------------------------------------------

def synthetic_bundle(patient_id: str = "patient-1") -> BundleFHIR:
    """Generate and parse a complete patient bundle into a BundleFHIR object."""
    from .bundle import parse_bundle
    return parse_bundle(generate_patient_bundle(patient_id))


def synthetic_malformed_bundle() -> BundleFHIR:
    """Generate and parse a malformed bundle."""
    from .bundle import parse_bundle
    return parse_bundle(generate_malformed_bundle())
