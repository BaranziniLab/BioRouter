"""
Patient Timeline Builder.

Merges encounters, observations, conditions, procedures, and medication
requests into a single chronological event stream, each tagged with a
standardised event type and sortable by datetime.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from typing import Optional

from .bundle import BundleFHIR
from .resources import (
    Condition,
    Encounter,
    FHIRResource,
    Observation,
    Procedure,
    MedicationRequest,
    Patient,
)


class EventType(Enum):
    """Canonical event types on the patient timeline."""
    ENCOUNTER = "encounter"
    OBSERVATION = "observation"
    CONDITION = "condition"
    PROCEDURE = "procedure"
    MEDICATION = "medication"
    UNKNOWN = "unknown"


@dataclass
class TimelineEvent:
    """A single event on the patient timeline."""
    event_type: EventType
    timestamp: datetime | None
    resource_type: str
    resource_id: str | None
    display: str
    details: dict = field(default_factory=dict)
    _sort_key: str = field(default="", repr=False)

    def __post_init__(self):
        # Stable sort key: timestamp then type then id
        # None timestamps sort LAST (use a very late date)
        if self.timestamp is not None:
            ts = self.timestamp.isoformat()
        else:
            ts = "9999-12-31T23:59:59"
        self._sort_key = f"{ts}|{self.event_type.value}|{self.resource_id or ''}"

    def __lt__(self, other: "TimelineEvent") -> bool:
        if not isinstance(other, TimelineEvent):
            return NotImplemented
        return self._sort_key < other._sort_key

    def __le__(self, other: "TimelineEvent") -> bool:
        if not isinstance(other, TimelineEvent):
            return NotImplemented
        return self._sort_key <= other._sort_key

    def __repr__(self) -> str:
        ts = self.timestamp.isoformat() if self.timestamp else "None"
        return f"TimelineEvent({self.event_type.value}, {ts}, {self.display!r})"


# ---------------------------------------------------------------------------
# Extraction helpers
# ---------------------------------------------------------------------------

def _encounter_event(enc: Encounter) -> TimelineEvent:
    """Convert an Encounter resource to a TimelineEvent."""
    class_display = enc.display_class
    codes = [t.first_display or t.first_code or "" for t in enc.type if t]
    label = class_display or (", ".join(codes) if codes else "Encounter")
    return TimelineEvent(
        event_type=EventType.ENCOUNTER,
        timestamp=enc.start_date,
        resource_type="Encounter",
        resource_id=enc.id,
        display=label,
        details={
            "status": enc.status,
            "class": class_display,
            "types": codes,
            "end_date": enc.end_date.isoformat() if enc.end_date else None,
        },
    )


def _observation_event(obs: Observation) -> TimelineEvent:
    code = obs.display_code or "Observation"
    value = obs.display_value
    display = f"{code}: {value}" if value else code
    return TimelineEvent(
        event_type=EventType.OBSERVATION,
        timestamp=obs.effective_date,
        resource_type="Observation",
        resource_id=obs.id,
        display=display,
        details={
            "code": code,
            "value": value,
            "status": obs.status,
            "numeric_value": obs.numeric_value,
        },
    )


def _condition_event(cond: Condition) -> TimelineEvent:
    code = cond.display_code or "Condition"
    status_code = cond.clinicalStatus.first_code if cond.clinicalStatus else ""
    display = f"{code} [{status_code}]" if status_code else code
    return TimelineEvent(
        event_type=EventType.CONDITION,
        timestamp=cond.onset_date,
        resource_type="Condition",
        resource_id=cond.id,
        display=display,
        details={
            "code": code,
            "clinical_status": status_code,
            "verification": (
                cond.verificationStatus.first_code if cond.verificationStatus else ""
            ),
            "severity": (
                cond.severity.first_display if cond.severity else ""
            ),
        },
    )


def _procedure_event(proc: Procedure) -> TimelineEvent:
    code = proc.display_code or "Procedure"
    status = proc.status or ""
    display = f"{code} [{status}]" if status else code
    return TimelineEvent(
        event_type=EventType.PROCEDURE,
        timestamp=proc.performed_date,
        resource_type="Procedure",
        resource_id=proc.id,
        display=display,
        details={
            "code": code,
            "status": status,
            "outcome": (
                proc.outcome.first_display if proc.outcome else ""
            ),
        },
    )


def _medication_event(med: MedicationRequest) -> TimelineEvent:
    name = med.display_medication or "Medication"
    status = med.status or ""
    display = f"{name} [{status}]" if status else name
    return TimelineEvent(
        event_type=EventType.MEDICATION,
        timestamp=med.authored_date,
        resource_type="MedicationRequest",
        resource_id=med.id,
        display=display,
        details={
            "medication": name,
            "status": status,
            "dosage": med.dosage_text,
            "intent": med.intent or "",
        },
    )


_EVENT_BUILDERS = {
    "Encounter": _encounter_event,
    "Observation": _observation_event,
    "Condition": _condition_event,
    "Procedure": _procedure_event,
    "MedicationRequest": _medication_event,
}


# ---------------------------------------------------------------------------
# Timeline
# ---------------------------------------------------------------------------

@dataclass
class PatientTimeline:
    """A sorted chronological stream of events for a single patient."""

    patient: Patient | None
    events: list[TimelineEvent] = field(default_factory=list)

    # Convenience properties
    @property
    def sorted_events(self) -> list[TimelineEvent]:
        return sorted(self.events)

    @property
    def encounters(self) -> list[TimelineEvent]:
        return sorted(e for e in self.events if e.event_type == EventType.ENCOUNTER)

    @property
    def observations(self) -> list[TimelineEvent]:
        return sorted(e for e in self.events if e.event_type == EventType.OBSERVATION)

    @property
    def conditions(self) -> list[TimelineEvent]:
        return sorted(e for e in self.events if e.event_type == EventType.CONDITION)

    @property
    def procedures(self) -> list[TimelineEvent]:
        return sorted(e for e in self.events if e.event_type == EventType.PROCEDURE)

    @property
    def medications(self) -> list[TimelineEvent]:
        return sorted(e for e in self.events if e.event_type == EventType.MEDICATION)

    @property
    def date_range(self) -> tuple[datetime | None, datetime | None]:
        dates = [e.timestamp for e in self.events if e.timestamp is not None]
        if not dates:
            return (None, None)
        return (min(dates), max(dates))

    @property
    def event_type_counts(self) -> dict[str, int]:
        counts: dict[str, int] = {}
        for e in self.events:
            counts[e.event_type.value] = counts.get(e.event_type.value, 0) + 1
        return counts

    def filter_by_type(self, event_type: EventType) -> list[TimelineEvent]:
        return sorted(e for e in self.events if e.event_type == event_type)

    def filter_by_date_range(
        self, start: datetime | None = None, end: datetime | None = None
    ) -> list[TimelineEvent]:
        result = []
        for e in sorted(self.events):
            if e.timestamp is None:
                continue
            if start and e.timestamp < start:
                continue
            if end and e.timestamp > end:
                continue
            result.append(e)
        return result

    def __len__(self) -> int:
        return len(self.events)

    def __iter__(self):
        return iter(self.sorted_events)

    def __repr__(self) -> str:
        patient_name = self.patient.display_name if self.patient else "Unknown"
        return (
            f"PatientTimeline(patient={patient_name!r}, "
            f"events={len(self.events)}, "
            f"types={self.event_type_counts})"
        )


# ---------------------------------------------------------------------------
# Builder
# ---------------------------------------------------------------------------

def build_timeline(bundle: BundleFHIR) -> PatientTimeline:
    """Build a chronological patient timeline from a FHIR Bundle.

    Extracts all supported resource types, converts each to a TimelineEvent,
    and returns them sorted by timestamp.
    """
    patient = bundle.get_patient()
    events: list[TimelineEvent] = []

    for entry in bundle:
        if entry.resource is None:
            continue
        builder = _EVENT_BUILDERS.get(entry.resource.resourceType)
        if builder:
            try:
                event = builder(entry.resource)
                events.append(event)
            except Exception:
                # Skip resources that fail to convert
                continue

    timeline = PatientTimeline(patient=patient, events=events)
    return timeline


def build_timeline_from_resources(
    resources: list[FHIRResource], patient: Patient | None = None
) -> PatientTimeline:
    """Build a timeline directly from a list of resources."""
    events: list[TimelineEvent] = []
    for resource in resources:
        builder = _EVENT_BUILDERS.get(resource.resourceType)
        if builder:
            try:
                events.append(builder(resource))
            except Exception:
                continue

    if patient is None:
        for r in resources:
            if isinstance(r, Patient):
                patient = r
                break

    return PatientTimeline(patient=patient, events=events)
