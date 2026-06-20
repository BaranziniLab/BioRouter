"""
FHIR Validation.

Validates FHIR resources for:
  - Required fields per resource type
  - Value-set membership for coded fields
  - Reference integrity within a bundle
  - Format constraints (date formats, etc.)

Returns a list of ValidationError objects with helpful messages.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Any, Optional

from .bundle import BundleFHIR
from .resources import (
    FHIRResource,
    Patient,
    Encounter,
    Observation,
    Condition,
    MedicationRequest,
    Procedure,
    AllergyIntolerance,
    Reference,
)


# ---------------------------------------------------------------------------
# Error model
# ---------------------------------------------------------------------------

@dataclass
class ValidationError:
    """A single validation error or warning."""

    resource_type: str
    resource_id: str | None
    field_path: str
    severity: str       # "error" or "warning"
    message: str

    def __str__(self) -> str:
        rid = self.resource_id or "unknown"
        return f"[{self.severity.upper()}] {self.resource_type}/{rid} – {self.field_path}: {self.message}"

    def __repr__(self) -> str:
        return f"ValidationError({self.resource_type!r}, {self.field_path!r}, {self.message!r})"


@dataclass
class ValidationResult:
    """Aggregate validation result for a bundle or resource list."""

    errors: list[ValidationError] = field(default_factory=list)

    @property
    def error_count(self) -> int:
        return sum(1 for e in self.errors if e.severity == "error")

    @property
    def warning_count(self) -> int:
        return sum(1 for e in self.errors if e.severity == "warning")

    @property
    def is_valid(self) -> bool:
        return self.error_count == 0

    def add(self, error: ValidationError) -> None:
        self.errors.append(error)

    def add_all(self, errors: list[ValidationError]) -> None:
        self.errors.extend(errors)

    def __str__(self) -> str:
        if self.is_valid:
            return "Validation passed (0 errors, 0 warnings)"
        return (
            f"Validation failed: {self.error_count} error(s), "
            f"{self.warning_count} warning(s)"
        )

    def __repr__(self) -> str:
        return f"ValidationResult(errors={self.error_count}, warnings={self.warning_count})"

    def __iter__(self):
        return iter(self.errors)


# ---------------------------------------------------------------------------
# Value sets
# ---------------------------------------------------------------------------

VALID_GENDER = {"male", "female", "other", "unknown"}

VALID_ENCOUNTER_STATUS = {
    "planned", "arrived", "triaged", "in-progress",
    "onleave", "finished", "cancelled", "entered-in-error",
}

VALID_OBSERVATION_STATUS = {
    "registered", "preliminary", "final", "amended",
    "corrected", "cancelled", "entered-in-error",
}

VALID_CONDITION_CLINICAL_STATUS = {
    "active", "recurrence", "relapse",
    "inactive", "remission", "resolved",
}

VALID_CONDITION_VERIFICATION_STATUS = {
    "confirmed", "provisional", "differential",
    "refuted", "unconfirmed",
}

VALID_MEDICATION_REQUEST_STATUS = {
    "active", "on-hold", "cancelled", "completed",
    "entered-in-error", "stopped", "draft", "unknown",
}

VALID_MEDICATION_REQUEST_INTENT = {
    "proposal", "plan", "order", "original-order",
    "reflex-order", "filler-order", "instance-order",
    "option",
}

VALID_PROCEDURE_STATUS = {
    "preparation", "in-progress", "not-done", "on-hold",
    "stopped", "completed", "entered-in-error", "unknown",
}

VALID_ALLERGY_CLINICAL_STATUS = {"active", "inactive", "resolved"}

VALID_ALLERGY_VERIFICATION_STATUS = {
    "confirmed", "unconfirmed", "refuted", "provisional",
}

VALID_ALLERGY_CRITICALITY = {"low", "high", "unable-to-assess"}

VALID_ALLERGY_CATEGORY = {"food", "medication", "environment", "biologic"}


# ---------------------------------------------------------------------------
# Regex patterns for format validation
# ---------------------------------------------------------------------------

RE_DATE = re.compile(r"^\d{4}(-\d{2}(-\d{2})?)?$")
RE_DATETIME = re.compile(
    r"^\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}(:\d{2})?(Z|[+-]\d{2}:\d{2})?)?$"
)
RE_ID = re.compile(r"^[A-Za-z0-9\-\.]{1,64}$")


# ---------------------------------------------------------------------------
# Validators
# ---------------------------------------------------------------------------

def _err(
    rtype: str, rid: str | None, path: str, msg: str, severity: str = "error"
) -> ValidationError:
    return ValidationError(rtype, rid, path, severity, msg)


def _validate_date_field(
    rtype: str, rid: str | None, field_name: str, value: str | None
) -> list[ValidationError]:
    if value is None:
        return []
    if not RE_DATE.match(value):
        return [_err(rtype, rid, field_name, f"Invalid date format: {value!r} (expected YYYY, YYYY-MM, or YYYY-MM-DD)")]
    return []


def _validate_datetime_field(
    rtype: str, rid: str | None, field_name: str, value: str | None
) -> list[ValidationError]:
    if value is None:
        return []
    if not RE_DATETIME.match(value):
        return [_err(rtype, rid, field_name, f"Invalid dateTime format: {value!r}")]
    return []


def _validate_id_field(
    rtype: str, rid: str | None, field_name: str, value: str | None
) -> list[ValidationError]:
    if value is None:
        return []
    if not RE_ID.match(value):
        return [_err(rtype, rid, field_name, f"Invalid id: {value!r} (must be 1-64 chars, alphanumeric/hyphen/dot)")]
    return []


def _validate_value_set(
    rtype: str, rid: str | None, field_name: str,
    value: str | None, valid_values: set[str], severity: str = "warning"
) -> list[ValidationError]:
    if value is None:
        return []
    if value not in valid_values:
        return [_err(
            rtype, rid, field_name,
            f"Value {value!r} not in expected value set: {sorted(valid_values)}",
            severity=severity,
        )]
    return []


# ---------------------------------------------------------------------------
# Per-resource-type validators
# ---------------------------------------------------------------------------

def _validate_patient(patient: Patient) -> list[ValidationError]:
    errs: list[ValidationError] = []
    rt = "Patient"
    rid = patient.id

    # Required: id
    if not patient.id:
        errs.append(_err(rt, rid, "id", "Patient.id is required"))

    # Required: identifier
    if not patient.identifier:
        errs.append(_err(rt, rid, "identifier", "Patient.identifier is recommended (at least one identifier expected)"))

    # Required: name
    if not patient.name:
        errs.append(_err(rt, rid, "name", "Patient.name is required"))

    # Gender value set
    errs.extend(_validate_value_set(rt, rid, "gender", patient.gender, VALID_GENDER))

    # Date formats
    if patient.birthDate is not None:
        errs.extend(_validate_date_field(rt, rid, "birthDate", str(patient.birthDate)))

    # ID format
    errs.extend(_validate_id_field(rt, rid, "id", patient.id))

    return errs


def _validate_encounter(enc: Encounter) -> list[ValidationError]:
    errs: list[ValidationError] = []
    rt = "Encounter"
    rid = enc.id

    if not enc.id:
        errs.append(_err(rt, rid, "id", "Encounter.id is required"))

    # Required: status
    if not enc.status:
        errs.append(_err(rt, rid, "status", "Encounter.status is required"))
    else:
        errs.extend(_validate_value_set(rt, rid, "status", enc.status, VALID_ENCOUNTER_STATUS))

    # Required: class
    if enc.class_ is None:
        errs.append(_err(rt, rid, "class", "Encounter.class is required"))

    # Required: subject
    if enc.subject is None:
        errs.append(_err(rt, rid, "subject", "Encounter.subject is required (must reference a Patient)"))

    # Period format
    if enc.period:
        if enc.period.start is not None:
            errs.extend(_validate_datetime_field(rt, rid, "period.start", str(enc.period.start)))
        if enc.period.end is not None:
            errs.extend(_validate_datetime_field(rt, rid, "period.end", str(enc.period.end)))

    errs.extend(_validate_id_field(rt, rid, "id", enc.id))
    return errs


def _validate_observation(obs: Observation) -> list[ValidationError]:
    errs: list[ValidationError] = []
    rt = "Observation"
    rid = obs.id

    if not obs.id:
        errs.append(_err(rt, rid, "id", "Observation.id is required"))

    # Required: status
    if not obs.status:
        errs.append(_err(rt, rid, "status", "Observation.status is required"))
    else:
        errs.extend(_validate_value_set(rt, rid, "status", obs.status, VALID_OBSERVATION_STATUS))

    # Required: code
    if obs.code is None:
        errs.append(_err(rt, rid, "code", "Observation.code is required (LOINC or other code)"))

    # Required: subject
    if obs.subject is None:
        errs.append(_err(rt, rid, "subject", "Observation.subject is required (must reference a Patient)"))

    # Must have at least one value
    has_value = any([
        obs.valueQuantity is not None,
        obs.valueCodeableConcept is not None,
        obs.valueString is not None,
        obs.valueBoolean is not None,
        obs.valueInteger is not None,
        obs.valueDateTime is not None,
        obs.component,  # component observations can hold values
    ])
    if not has_value:
        errs.append(_err(rt, rid, "value[x]", "Observation must have at least one value[x] or component", severity="warning"))

    errs.extend(_validate_id_field(rt, rid, "id", obs.id))
    return errs


def _validate_condition(cond: Condition) -> list[ValidationError]:
    errs: list[ValidationError] = []
    rt = "Condition"
    rid = cond.id

    if not cond.id:
        errs.append(_err(rt, rid, "id", "Condition.id is required"))

    # clinicalStatus value set
    if cond.clinicalStatus:
        cs = cond.clinicalStatus.first_code
        errs.extend(_validate_value_set(rt, rid, "clinicalStatus", cs, VALID_CONDITION_CLINICAL_STATUS))

    # verificationStatus value set
    if cond.verificationStatus:
        vs = cond.verificationStatus.first_code
        errs.extend(_validate_value_set(rt, rid, "verificationStatus", vs, VALID_CONDITION_VERIFICATION_STATUS))

    # Required: subject
    if cond.subject is None:
        errs.append(_err(rt, rid, "subject", "Condition.subject is required (must reference a Patient)"))

    # Recommended: code
    if cond.code is None:
        errs.append(_err(rt, rid, "code", "Condition.code is recommended", severity="warning"))

    errs.extend(_validate_id_field(rt, rid, "id", cond.id))
    return errs


def _validate_medication_request(med: MedicationRequest) -> list[ValidationError]:
    errs: list[ValidationError] = []
    rt = "MedicationRequest"
    rid = med.id

    if not med.id:
        errs.append(_err(rt, rid, "id", "MedicationRequest.id is required"))

    # Required: status
    if not med.status:
        errs.append(_err(rt, rid, "status", "MedicationRequest.status is required"))
    else:
        errs.extend(_validate_value_set(rt, rid, "status", med.status, VALID_MEDICATION_REQUEST_STATUS))

    # Required: intent
    if not med.intent:
        errs.append(_err(rt, rid, "intent", "MedicationRequest.intent is required"))
    else:
        errs.extend(_validate_value_set(rt, rid, "intent", med.intent, VALID_MEDICATION_REQUEST_INTENT))

    # Required: medication
    if med.medicationCodeableConcept is None and med.medicationReference is None:
        errs.append(_err(rt, rid, "medication[x]", "MedicationRequest requires medicationCodeableConcept or medicationReference"))

    # Required: subject
    if med.subject is None:
        errs.append(_err(rt, rid, "subject", "MedicationRequest.subject is required (must reference a Patient)"))

    errs.extend(_validate_id_field(rt, rid, "id", med.id))
    return errs


def _validate_procedure(proc: Procedure) -> list[ValidationError]:
    errs: list[ValidationError] = []
    rt = "Procedure"
    rid = proc.id

    if not proc.id:
        errs.append(_err(rt, rid, "id", "Procedure.id is required"))

    # Required: status
    if not proc.status:
        errs.append(_err(rt, rid, "status", "Procedure.status is required"))
    else:
        errs.extend(_validate_value_set(rt, rid, "status", proc.status, VALID_PROCEDURE_STATUS))

    # Required: subject
    if proc.subject is None:
        errs.append(_err(rt, rid, "subject", "Procedure.subject is required (must reference a Patient)"))

    errs.extend(_validate_id_field(rt, rid, "id", proc.id))
    return errs


def _validate_allergy_intolerance(ai: AllergyIntolerance) -> list[ValidationError]:
    errs: list[ValidationError] = []
    rt = "AllergyIntolerance"
    rid = ai.id

    if not ai.id:
        errs.append(_err(rt, rid, "id", "AllergyIntolerance.id is required"))

    # clinicalStatus
    if ai.clinicalStatus:
        cs = ai.clinicalStatus.first_code
        errs.extend(_validate_value_set(rt, rid, "clinicalStatus", cs, VALID_ALLERGY_CLINICAL_STATUS))

    # verificationStatus
    if ai.verificationStatus:
        vs = ai.verificationStatus.first_code
        errs.extend(_validate_value_set(rt, rid, "verificationStatus", vs, VALID_ALLERGY_VERIFICATION_STATUS))

    # criticality
    errs.extend(_validate_value_set(rt, rid, "criticality", ai.criticality, VALID_ALLERGY_CRITICALITY))

    # category
    for cat in ai.category:
        errs.extend(_validate_value_set(rt, rid, "category", cat, VALID_ALLERGY_CATEGORY))

    # Required: patient
    if ai.patient is None:
        errs.append(_err(rt, rid, "patient", "AllergyIntolerance.patient is required (must reference a Patient)"))

    errs.extend(_validate_id_field(rt, rid, "id", ai.id))
    return errs


_RESOURCE_VALIDATORS = {
    "Patient": _validate_patient,
    "Encounter": _validate_encounter,
    "Observation": _validate_observation,
    "Condition": _validate_condition,
    "MedicationRequest": _validate_medication_request,
    "Procedure": _validate_procedure,
    "AllergyIntolerance": _validate_allergy_intolerance,
}


# ---------------------------------------------------------------------------
# Reference integrity
# ---------------------------------------------------------------------------

def _validate_references(bundle: BundleFHIR) -> list[ValidationError]:
    """Check that all internal references in resources resolve within the bundle."""
    errs: list[ValidationError] = []

    for entry in bundle:
        if entry.resource is None:
            continue

        resource = entry.resource
        rtype = resource.resourceType
        rid = resource.id

        # Collect all Reference objects in this resource
        refs = _extract_references(resource)
        for field_name, ref in refs:
            if ref.reference is None:
                continue
            # Skip external references (urn:uuid:, http:, etc.)
            ref_str = ref.reference
            if ref_str.startswith("urn:") or ref_str.startswith("http"):
                continue
            # Check resolution
            resolved = bundle.resolve_reference(ref_str)
            if resolved is None:
                errs.append(_err(
                    rtype, rid, field_name,
                    f"Reference {ref_str!r} cannot be resolved within this bundle",
                ))

    return errs


def _extract_references(resource: FHIRResource) -> list[tuple[str, Reference]]:
    """Extract all (field_name, Reference) pairs from a resource."""
    pairs: list[tuple[str, Reference]] = []

    def _add(name: str, ref: Any) -> None:
        if isinstance(ref, Reference):
            pairs.append((name, ref))

    if isinstance(resource, Patient):
        pass  # Patient has no reference fields to validate here
    elif isinstance(resource, Encounter):
        _add("subject", resource.subject)
    elif isinstance(resource, Observation):
        _add("subject", resource.subject)
        _add("encounter", resource.encounter)
    elif isinstance(resource, Condition):
        _add("subject", resource.subject)
        _add("encounter", resource.encounter)
        _add("recorder", resource.recorder)
    elif isinstance(resource, MedicationRequest):
        _add("subject", resource.subject)
        _add("encounter", resource.encounter)
        _add("medicationReference", resource.medicationReference)
        _add("requester", resource.requester)
    elif isinstance(resource, Procedure):
        _add("subject", resource.subject)
        _add("encounter", resource.encounter)
    elif isinstance(resource, AllergyIntolerance):
        _add("patient", resource.patient)
        _add("recorder", resource.recorder)

    return pairs


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

def validate_resource(resource: FHIRResource) -> ValidationResult:
    """Validate a single FHIR resource."""
    result = ValidationResult()
    validator = _RESOURCE_VALIDATORS.get(resource.resourceType)
    if validator:
        result.add_all(validator(resource))
    else:
        result.add(_err(
            resource.resourceType, resource.id, "resourceType",
            f"No validator defined for resource type: {resource.resourceType}",
            severity="warning",
        ))
    return result


def validate_bundle(bundle: BundleFHIR) -> ValidationResult:
    """Validate all resources in a bundle and check reference integrity."""
    result = ValidationResult()

    for entry in bundle:
        if entry.resource is None:
            continue
        result.add_all(validate_resource(entry.resource).errors)

    # Reference integrity
    result.add_all(_validate_references(bundle))

    return result
