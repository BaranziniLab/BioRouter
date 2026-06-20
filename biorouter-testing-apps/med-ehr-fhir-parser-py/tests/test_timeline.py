"""
Tests for timeline.py — Patient timeline building and ordering.
"""

import pytest
from datetime import datetime

from fhir_parser.bundle import BundleFHIR
from fhir_parser.timeline import (
    build_timeline,
    build_timeline_from_resources,
    PatientTimeline,
    TimelineEvent,
    EventType,
)
from fhir_parser.resources import Patient, Observation, Condition, Encounter
from fhir_parser.synthetic import (
    generate_patient_bundle,
    generate_simple_bundle,
    generate_observation,
    generate_encounter,
)


class TestTimelineEvent:
    def test_sorting(self):
        e1 = TimelineEvent(EventType.ENCOUNTER, datetime(2024, 1, 1), "Encounter", "e1", "Visit 1")
        e2 = TimelineEvent(EventType.OBSERVATION, datetime(2024, 1, 15), "Observation", "o1", "HR: 72")
        e3 = TimelineEvent(EventType.CONDITION, datetime(2024, 1, 10), "Condition", "c1", "Diabetes")
        events = sorted([e2, e3, e1])
        assert events[0] == e1
        assert events[1] == e3
        assert events[2] == e2

    def test_none_timestamp(self):
        e1 = TimelineEvent(EventType.CONDITION, None, "Condition", "c1", "Unknown onset")
        e2 = TimelineEvent(EventType.ENCOUNTER, datetime(2024, 1, 1), "Encounter", "e1", "Visit")
        events = sorted([e2, e1])
        # None timestamps come after dated events (sorted to end by sort key)
        assert events[0] == e2
        assert events[1] == e1

    def test_lt(self):
        e1 = TimelineEvent(EventType.ENCOUNTER, datetime(2024, 1, 1), "Encounter", "e1", "Visit 1")
        e2 = TimelineEvent(EventType.OBSERVATION, datetime(2024, 6, 1), "Observation", "o1", "HR: 72")
        assert e1 < e2
        assert not e2 < e1

    def test_repr(self):
        e = TimelineEvent(EventType.ENCOUNTER, datetime(2024, 1, 1), "Encounter", "e1", "Visit")
        assert "encounter" in repr(e)


class TestBuildTimeline:
    def test_build_from_simple_bundle(self):
        raw = generate_simple_bundle()
        bundle = BundleFHIR.from_dict(raw)
        timeline = build_timeline(bundle)
        assert timeline.patient is not None
        assert timeline.patient.id == "simple-patient"
        assert len(timeline.events) >= 1  # At least the encounter

    def test_build_from_full_bundle(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        timeline = build_timeline(bundle)
        assert timeline.patient is not None
        assert len(timeline.events) > 0

    def test_events_are_sorted(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        timeline = build_timeline(bundle)
        sorted_events = timeline.sorted_events
        for i in range(len(sorted_events) - 1):
            assert sorted_events[i] <= sorted_events[i + 1]

    def test_event_type_filters(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        timeline = build_timeline(bundle)
        assert len(timeline.encounters) == 3
        assert len(timeline.observations) == 12
        assert len(timeline.conditions) == 4
        assert len(timeline.medications) == 4
        assert len(timeline.procedures) == 2

    def test_event_type_counts(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        timeline = build_timeline(bundle)
        counts = timeline.event_type_counts
        assert counts.get("encounter", 0) == 3
        assert counts.get("observation", 0) == 12
        assert counts.get("condition", 0) == 4
        assert counts.get("medication", 0) == 4

    def test_date_range(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        timeline = build_timeline(bundle)
        start, end = timeline.date_range
        assert start is not None
        assert end is not None
        assert start < end

    def test_filter_by_type(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        timeline = build_timeline(bundle)
        encounters = timeline.filter_by_type(EventType.ENCOUNTER)
        assert len(encounters) == 3
        for e in encounters:
            assert e.event_type == EventType.ENCOUNTER

    def test_filter_by_date_range(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        timeline = build_timeline(bundle)
        filtered = timeline.filter_by_date_range(
            start=datetime(2024, 1, 1),
            end=datetime(2024, 2, 1),
        )
        # Should only include January events
        for e in filtered:
            if e.timestamp:
                assert e.timestamp.year == 2024
                assert e.timestamp.month == 1

    def test_empty_bundle(self):
        raw = {
            "resourceType": "Bundle",
            "type": "collection",
            "entry": [],
        }
        bundle = BundleFHIR.from_dict(raw)
        timeline = build_timeline(bundle)
        assert len(timeline.events) == 0
        assert timeline.patient is None
        assert timeline.date_range == (None, None)

    def test_timeline_repr(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        timeline = build_timeline(bundle)
        r = repr(timeline)
        assert "PatientTimeline" in r
        assert "Doe" in r

    def test_timeline_len(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        timeline = build_timeline(bundle)
        assert len(timeline) == len(timeline.events)

    def test_timeline_iter(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        timeline = build_timeline(bundle)
        events = list(timeline)
        assert len(events) == len(timeline.events)
        # Iter should be sorted
        for i in range(len(events) - 1):
            assert events[i] <= events[i + 1]


class TestBuildTimelineFromResources:
    def test_from_resource_list(self):
        resources = [
            Patient(id="p1", resourceType="Patient", gender="female"),
            Observation(
                id="o1",
                resourceType="Observation",
                status="final",
                code=None,
                effectiveDateTime=datetime(2024, 1, 15),
                valueQuantity=None,
            ),
        ]
        timeline = build_timeline_from_resources(resources, patient=resources[0])
        assert timeline.patient is not None
        assert timeline.patient.id == "p1"

    def test_infers_patient(self):
        resources = [
            Patient(id="p2", resourceType="Patient", gender="male"),
        ]
        timeline = build_timeline_from_resources(resources)
        assert timeline.patient is not None
        assert timeline.patient.id == "p2"

    def test_encounter_events(self):
        """Encounters should have proper display labels."""
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        timeline = build_timeline(bundle)
        for e in timeline.encounters:
            assert e.display  # Should have a non-empty display
            assert e.event_type == EventType.ENCOUNTER

    def test_observation_events(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        timeline = build_timeline(bundle)
        for e in timeline.observations:
            assert e.event_type == EventType.OBSERVATION
            assert "code" in e.details
            assert "value" in e.details

    def test_condition_events(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        timeline = build_timeline(bundle)
        for e in timeline.conditions:
            assert e.event_type == EventType.CONDITION
            assert "clinical_status" in e.details

    def test_medication_events(self):
        raw = generate_patient_bundle()
        bundle = BundleFHIR.from_dict(raw)
        timeline = build_timeline(bundle)
        for e in timeline.medications:
            assert e.event_type == EventType.MEDICATION
            assert "medication" in e.details
            assert "status" in e.details
