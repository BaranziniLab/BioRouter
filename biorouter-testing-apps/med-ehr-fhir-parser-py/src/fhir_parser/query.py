"""
Query Engine for FHIR resources.

Provides high-level query functions over a FHIR Bundle:
  - Active conditions
  - Latest vitals
  - Medications on a date
  - Observation trends

All queries accept a BundleFHIR and return structured results.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import date, datetime, timedelta
from typing import Optional

from .bundle import BundleFHIR
from .resources import (
    Condition,
    Encounter,
    FHIRResource,
    MedicationRequest,
    Observation,
    Patient,
    Procedure,
    AllergyIntolerance,
)


# ---------------------------------------------------------------------------
# Result dataclasses
# ---------------------------------------------------------------------------

@dataclass
class ActiveConditionResult:
    """A single active condition."""
    condition_id: str | None
    code_display: str
    clinical_status: str
    verification_status: str
    severity: str
    onset_date: datetime | None
    raw: Condition

    def __repr__(self) -> str:
        return f"ActiveCondition({self.code_display!r}, status={self.clinical_status!r})"


@dataclass
class LatestVitalResult:
    """The most recent value for a vital sign category."""
    code_display: str
    value: str
    numeric_value: float | None
    unit: str
    effective_date: datetime | None
    status: str
    observation_id: str | None
    raw: Observation

    def __repr__(self) -> str:
        return f"LatestVital({self.code_display!r}={self.value!r})"


@dataclass
class MedicationOnDateResult:
    """A medication active on a specific date."""
    medication_display: str
    status: str
    authored_on: datetime | None
    dosage: str
    medication_request_id: str | None
    raw: MedicationRequest

    def __repr__(self) -> str:
        return f"MedicationOnDate({self.medication_display!r}, status={self.status!r})"


@dataclass
class ObservationTrendPoint:
    """A single data point in an observation trend."""
    effective_date: datetime | None
    numeric_value: float | None
    display_value: str
    observation_id: str | None


@dataclass
class ObservationTrendResult:
    """A trend of observation values over time."""
    code_display: str
    points: list[ObservationTrendPoint] = field(default_factory=list)
    unit: str = ""

    @property
    def count(self) -> int:
        return len(self.points)

    @property
    def latest_value(self) -> float | None:
        dated = [p for p in self.points if p.effective_date is not None]
        if not dated:
            return None
        latest = max(dated, key=lambda p: p.effective_date)  # type: ignore[arg-type]
        return latest.numeric_value

    @property
    def earliest_value(self) -> float | None:
        dated = [p for p in self.points if p.effective_date is not None]
        if not dated:
            return None
        earliest = min(dated, key=lambda p: p.effective_date)  # type: ignore[arg-type]
        return earliest.numeric_value

    @property
    def min_value(self) -> float | None:
        vals = [p.numeric_value for p in self.points if p.numeric_value is not None]
        return min(vals) if vals else None

    @property
    def max_value(self) -> float | None:
        vals = [p.numeric_value for p in self.points if p.numeric_value is not None]
        return max(vals) if vals else None

    @property
    def mean_value(self) -> float | None:
        vals = [p.numeric_value for p in self.points if p.numeric_value is not None]
        return sum(vals) / len(vals) if vals else None

    def __repr__(self) -> str:
        return f"ObservationTrend({self.code_display!r}, count={self.count})"


# ---------------------------------------------------------------------------
# Vital sign LOINC codes (common)
# ---------------------------------------------------------------------------

VITAL_SIGN_CODES: dict[str, set[str]] = {
    "blood_pressure": {"85354-9"},
    "heart_rate": {"8867-4"},
    "respiratory_rate": {"9279-1"},
    "body_temperature": {"8310-5"},
    "body_weight": {"29463-7"},
    "body_height": {"8302-2"},
    "bmi": {"39156-5"},
    "oxygen_saturation": {"2708-6", "59408-5"},
    "pulse_oximetry": {"59408-5"},
}

# Expand to a flat set for fast lookup
_ALL_VITAL_CODES: set[str] = set()
for codes in VITAL_SIGN_CODES.values():
    _ALL_VITAL_CODES.update(codes)


def is_vital_sign(obs: Observation) -> bool:
    """Check if an observation is a vital sign by LOINC code."""
    if obs.code is None:
        return False
    for coding in obs.code.coding:
        if coding.system and "loinc" in coding.system.lower():
            if coding.code in _ALL_VITAL_CODES:
                return True
        if coding.code and coding.code in _ALL_VITAL_CODES:
            return True
    # Also check category for vital-signs
    for cat in obs.category:
        for coding in cat.coding:
            if coding.code == "vital-signs":
                return True
    return False


# ---------------------------------------------------------------------------
# Query functions
# ---------------------------------------------------------------------------

def query_active_conditions(bundle: BundleFHIR) -> list[ActiveConditionResult]:
    """Return all conditions with an active clinical status.

    A condition is considered active if its clinicalStatus code is one of:
    active, recurrence, relapse.
    """
    results: list[ActiveConditionResult] = []
    for resource in bundle.get_resources_by_type("Condition"):
        assert isinstance(resource, Condition)
        cond: Condition = resource

        # Determine clinical status
        cs_code = cond.clinicalStatus.first_code if cond.clinicalStatus else ""
        active_codes = {"active", "recurrence", "relapse"}
        if cs_code not in active_codes:
            continue

        vs_code = cond.verificationStatus.first_code if cond.verificationStatus else ""
        severity = cond.severity.first_display if cond.severity else ""

        results.append(ActiveConditionResult(
            condition_id=cond.id,
            code_display=cond.display_code,
            clinical_status=cs_code,
            verification_status=vs_code,
            severity=severity,
            onset_date=cond.onset_date,
            raw=cond,
        ))

    # Sort by onset date (None last)
    results.sort(key=lambda r: r.onset_date or datetime.max)
    return results


def query_latest_vitals(
    bundle: BundleFHIR, *, codes: set[str] | None = None
) -> list[LatestVitalResult]:
    """Return the most recent vital-sign observation for each code.

    If *codes* is provided, restrict to those LOINC codes; otherwise
    all vital-sign observations are included.
    """
    observations: list[Observation] = []
    for resource in bundle.get_resources_by_type("Observation"):
        assert isinstance(resource, Observation)
        obs: Observation = resource

        # Filter to vital signs
        if not is_vital_sign(obs):
            continue

        # If specific codes requested, filter further
        if codes and obs.code:
            obs_codes = {c.code for c in obs.code.coding if c.code}
            if not obs_codes.intersection(codes):
                continue

        observations.append(obs)

    # Group by code, keep latest
    latest: dict[str, Observation] = {}
    for obs in observations:
        if obs.code is None:
            continue
        display = obs.display_code
        key = display or obs.id or ""
        if key not in latest:
            latest[key] = obs
        else:
            existing = latest[key]
            if obs.effective_date and existing.effective_date:
                if obs.effective_date > existing.effective_date:
                    latest[key] = obs
            elif obs.effective_date and not existing.effective_date:
                latest[key] = obs

    results: list[LatestVitalResult] = []
    for code_key, obs in sorted(latest.items()):
        vq = obs.valueQuantity
        results.append(LatestVitalResult(
            code_display=obs.display_code,
            value=obs.display_value,
            numeric_value=obs.numeric_value,
            unit=vq.unit if vq else "",
            effective_date=obs.effective_date,
            status=obs.status or "",
            observation_id=obs.id,
            raw=obs,
        ))

    return results


def query_medications_on_date(
    bundle: BundleFHIR, target_date: date
) -> list[MedicationOnDateResult]:
    """Return medications that are likely active on a given date.

    A medication is considered active if:
      - status == 'active'
      - authoredOn <= target_date
    """
    results: list[MedicationOnDateResult] = []
    for resource in bundle.get_resources_by_type("MedicationRequest"):
        assert isinstance(resource, MedicationRequest)
        med: MedicationRequest = resource

        if med.status != "active":
            continue

        authored = med.authored_date
        if authored is None:
            continue

        authored_date_only = authored.date()
        if authored_date_only > target_date:
            continue

        results.append(MedicationOnDateResult(
            medication_display=med.display_medication,
            status=med.status or "",
            authored_on=authored,
            dosage=med.dosage_text,
            medication_request_id=med.id,
            raw=med,
        ))

    # Sort by medication name
    results.sort(key=lambda r: r.medication_display.lower())
    return results


def query_observation_trends(
    bundle: BundleFHIR, *, code_filter: str | None = None
) -> list[ObservationTrendResult]:
    """Build observation trends grouped by code.

    If *code_filter* is provided (a display string or LOINC code),
    only observations matching that code are included.
    """
    observations: list[Observation] = []
    for resource in bundle.get_resources_by_type("Observation"):
        assert isinstance(resource, Observation)
        obs: Observation = resource

        # Must have a numeric value to be useful in trends
        if obs.numeric_value is None:
            continue

        # Apply code filter
        if code_filter:
            if obs.code is None:
                continue
            matched = False
            display = obs.display_code.lower()
            for coding in obs.code.coding:
                if coding.code and code_filter.lower() in coding.code.lower():
                    matched = True
                    break
            if not matched and code_filter.lower() not in display:
                continue

        observations.append(obs)

    # Group by display code
    groups: dict[str, list[Observation]] = {}
    for obs in observations:
        key = obs.display_code or "Unknown"
        groups.setdefault(key, []).append(obs)

    results: list[ObservationTrendResult] = []
    for code_key, obs_list in sorted(groups.items()):
        points: list[ObservationTrendPoint] = []
        unit = ""
        for obs in obs_list:
            vq = obs.valueQuantity
            if vq and vq.unit and not unit:
                unit = vq.unit
            points.append(ObservationTrendPoint(
                effective_date=obs.effective_date,
                numeric_value=obs.numeric_value,
                display_value=obs.display_value,
                observation_id=obs.id,
            ))
        # Sort points by date
        points.sort(key=lambda p: p.effective_date or datetime.min)

        results.append(ObservationTrendResult(
            code_display=code_key,
            points=points,
            unit=unit,
        ))

    return results


def query_allergy_intolerances(bundle: BundleFHIR) -> list[AllergyIntolerance]:
    """Return all active allergy intolerance resources."""
    results: list[AllergyIntolerance] = []
    for resource in bundle.get_resources_by_type("AllergyIntolerance"):
        assert isinstance(resource, AllergyIntolerance)
        ai: AllergyIntolerance = resource
        if ai.is_active:
            results.append(ai)
    return results


def query_encounters(
    bundle: BundleFHIR,
    *,
    status_filter: str | None = None,
    class_filter: str | None = None,
) -> list[Encounter]:
    """Return encounters, optionally filtered by status or class."""
    results: list[Encounter] = []
    for resource in bundle.get_resources_by_type("Encounter"):
        assert isinstance(resource, Encounter)
        enc: Encounter = resource

        if status_filter and enc.status != status_filter:
            continue
        if class_filter and enc.display_class != class_filter:
            continue

        results.append(enc)

    results.sort(key=lambda e: e.start_date or datetime.min)
    return results


def query_procedures(
    bundle: BundleFHIR, *, status_filter: str | None = None
) -> list[Procedure]:
    """Return procedures, optionally filtered by status."""
    results: list[Procedure] = []
    for resource in bundle.get_resources_by_type("Procedure"):
        assert isinstance(resource, Procedure)
        proc: Procedure = resource
        if status_filter and proc.status != status_filter:
            continue
        results.append(proc)
    results.sort(key=lambda p: p.performed_date or datetime.min)
    return results
