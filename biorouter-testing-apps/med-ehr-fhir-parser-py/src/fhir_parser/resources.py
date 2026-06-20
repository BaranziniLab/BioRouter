"""
FHIR R4 Resource Models.

Typed dataclass-based representations of FHIR R4 resources:
Patient, Encounter, Observation, Condition, MedicationRequest,
Procedure, AllergyIntolerance.

Each resource has:
  - from_dict(cls, data: dict) -> T  (parse from FHIR JSON)
  - to_dict() -> dict                 (serialize to FHIR JSON)
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field, fields
from datetime import date, datetime
from enum import Enum
from typing import Any, Optional, Type, TypeVar, get_type_hints


# ---------------------------------------------------------------------------
# FHIR primitive helpers
# ---------------------------------------------------------------------------

class FHIRDateTime:
    """Represents a FHIR dateTime — can be a full instant, date, or partial."""

    __slots__ = ("_raw",)

    def __init__(self, raw: str | None):
        self._raw = raw

    # ---- construction helpers ----

    @classmethod
    def from_value(cls, value: str | None) -> Optional["FHIRDateTime"]:
        if value is None:
            return None
        return cls(value)

    @classmethod
    def from_dict(cls, d: dict | None) -> Optional["FHIRDateTime"]:
        if d is None:
            return None
        return cls(d.get("dateTime") or d.get("value"))

    # ---- accessors ----

    @property
    def raw(self) -> str | None:
        return self._raw

    @property
    def year(self) -> int | None:
        if self._raw and len(self._raw) >= 4:
            return int(self._raw[:4])
        return None

    @property
    def month(self) -> int | None:
        if self._raw and len(self._raw) >= 7:
            return int(self._raw[5:7])
        return None

    @property
    def day(self) -> int | None:
        if self._raw and len(self._raw) >= 10:
            return int(self._raw[8:10])
        return None

    def to_date(self) -> date | None:
        """Best-effort conversion to a Python date."""
        if self._raw and len(self._raw) >= 10:
            try:
                return date.fromisoformat(self._raw[:10])
            except ValueError:
                return None
        return None

    def to_datetime(self) -> datetime | None:
        """Best-effort conversion to a Python datetime (always naive/UTC)."""
        if not self._raw:
            return None
        for fmt in ("%Y-%m-%dT%H:%M:%S%z", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d"):
            try:
                dt = datetime.strptime(self._raw, fmt)
                # Normalise: strip tzinfo so all datetimes are naive (UTC assumed)
                if dt.tzinfo is not None:
                    dt = dt.replace(tzinfo=None)
                return dt
            except ValueError:
                continue
        return None

    def __str__(self) -> str:
        return self._raw or ""

    def __repr__(self) -> str:
        return f"FHIRDateTime({self._raw!r})"

    def __eq__(self, other: object) -> bool:
        if isinstance(other, FHIRDateTime):
            return self._raw == other._raw
        if isinstance(other, str):
            return self._raw == other
        return NotImplemented

    def __hash__(self) -> int:
        return hash(self._raw)


class FHIRDate:
    """FHIR date (YYYY or YYYY-MM or YYYY-MM-DD)."""

    __slots__ = ("_raw",)

    def __init__(self, raw: str | None):
        self._raw = raw

    @classmethod
    def from_value(cls, value: str | None) -> Optional["FHIRDate"]:
        if value is None:
            return None
        return cls(value)

    @property
    def raw(self) -> str | None:
        return self._raw

    def to_date(self) -> date | None:
        if self._raw and len(self._raw) >= 10:
            try:
                return date.fromisoformat(self._raw[:10])
            except ValueError:
                return None
        return None

    def __str__(self) -> str:
        return self._raw or ""

    def __repr__(self) -> str:
        return f"FHIRDate({self._raw!r})"

    def __eq__(self, other: object) -> bool:
        if isinstance(other, FHIRDate):
            return self._raw == other._raw
        if isinstance(other, str):
            return self._raw == other
        return NotImplemented

    def __hash__(self) -> int:
        return hash(self._raw)


# ---------------------------------------------------------------------------
# FHIR Reference
# ---------------------------------------------------------------------------

@dataclass
class Reference:
    """A FHIR reference — e.g. Patient/123 or a display-only reference."""

    reference: str | None = None
    display: str | None = None
    type: str | None = None

    @classmethod
    def from_dict(cls, d: dict | None) -> Optional["Reference"]:
        if d is None:
            return None
        return cls(
            reference=d.get("reference"),
            display=d.get("display"),
            type=d.get("type"),
        )

    def to_dict(self) -> dict:
        d: dict[str, Any] = {}
        if self.reference is not None:
            d["reference"] = self.reference
        if self.display is not None:
            d["display"] = self.display
        if self.type is not None:
            d["type"] = self.type
        return d

    @property
    def resource_type(self) -> str | None:
        """Return the resource type part of the reference string (e.g. 'Patient')."""
        if self.reference and "/" in self.reference:
            return self.reference.split("/")[0]
        return None

    @property
    def resource_id(self) -> str | None:
        """Return the id part of the reference string."""
        if self.reference and "/" in self.reference:
            return self.reference.split("/", 1)[1]
        return None

    def __repr__(self) -> str:
        return f"Reference({self.reference!r})"


# ---------------------------------------------------------------------------
# FHIR CodeableConcept
# ---------------------------------------------------------------------------

@dataclass
class Coding:
    system: str | None = None
    version: str | None = None
    code: str | None = None
    display: str | None = None
    userSelected: bool | None = None

    @classmethod
    def from_dict(cls, d: dict | None) -> Optional["Coding"]:
        if d is None:
            return None
        return cls(
            system=d.get("system"),
            version=d.get("version"),
            code=d.get("code"),
            display=d.get("display"),
            userSelected=d.get("userSelected"),
        )

    def to_dict(self) -> dict:
        return {k: v for k, v in {
            "system": self.system,
            "version": self.version,
            "code": self.code,
            "display": self.display,
            "userSelected": self.userSelected,
        }.items() if v is not None}

    def __repr__(self) -> str:
        return f"Coding(system={self.system!r}, code={self.code!r})"


@dataclass
class CodeableConcept:
    coding: list[Coding] = field(default_factory=list)
    text: str | None = None

    @classmethod
    def from_dict(cls, d: dict | None) -> Optional["CodeableConcept"]:
        if d is None:
            return None
        return cls(
            coding=[Coding.from_dict(c) for c in d.get("coding", []) if c],
            text=d.get("text"),
        )

    def to_dict(self) -> dict:
        d: dict[str, Any] = {}
        if self.coding:
            d["coding"] = [c.to_dict() for c in self.coding]
        if self.text is not None:
            d["text"] = self.text
        return d

    @property
    def first_code(self) -> str | None:
        """Convenience: return the first coding's code, or the text."""
        if self.coding and self.coding[0].code:
            return self.coding[0].code
        return self.text

    @property
    def first_display(self) -> str | None:
        if self.coding and self.coding[0].display:
            return self.coding[0].display
        return self.text

    def has_code(self, system: str, code: str) -> bool:
        return any(c.system == system and c.code == code for c in self.coding)

    def __repr__(self) -> str:
        return f"CodeableConcept(text={self.text!r})"


# ---------------------------------------------------------------------------
# FHIR Quantity
# ---------------------------------------------------------------------------

@dataclass
class Quantity:
    value: float | None = None
    comparator: str | None = None  # <, <=, >=, >
    unit: str | None = None
    system: str | None = None
    code: str | None = None

    @classmethod
    def from_dict(cls, d: dict | None) -> Optional["Quantity"]:
        if d is None:
            return None
        return cls(
            value=d.get("value"),
            comparator=d.get("comparator"),
            unit=d.get("unit"),
            system=d.get("system"),
            code=d.get("code"),
        )

    def to_dict(self) -> dict:
        return {k: v for k, v in {
            "value": self.value,
            "comparator": self.comparator,
            "unit": self.unit,
            "system": self.system,
            "code": self.code,
        }.items() if v is not None}

    def __repr__(self) -> str:
        return f"Quantity({self.value} {self.unit!r})"


# ---------------------------------------------------------------------------
# FHIR Period
# ---------------------------------------------------------------------------

@dataclass
class Period:
    start: FHIRDateTime | None = None
    end: FHIRDateTime | None = None

    @classmethod
    def from_dict(cls, d: dict | None) -> Optional["Period"]:
        if d is None:
            return None
        return cls(
            start=FHIRDateTime.from_value(d.get("start")),
            end=FHIRDateTime.from_value(d.get("end")),
        )

    def to_dict(self) -> dict:
        d: dict[str, Any] = {}
        if self.start is not None:
            d["start"] = str(self.start)
        if self.end is not None:
            d["end"] = str(self.end)
        return d

    def __repr__(self) -> str:
        return f"Period({self.start!r}, {self.end!r})"


# ---------------------------------------------------------------------------
# FHIR HumanName
# ---------------------------------------------------------------------------

@dataclass
class HumanName:
    use: str | None = None       # usual, official, temp, anonymous, old, maiden
    family: str | None = None
    given: list[str] = field(default_factory=list)
    prefix: list[str] = field(default_factory=list)
    suffix: list[str] = field(default_factory=list)
    text: str | None = None

    @classmethod
    def from_dict(cls, d: dict | None) -> Optional["HumanName"]:
        if d is None:
            return None
        return cls(
            use=d.get("use"),
            family=d.get("family"),
            given=d.get("given", []),
            prefix=d.get("prefix", []),
            suffix=d.get("suffix", []),
            text=d.get("text"),
        )

    def to_dict(self) -> dict:
        d: dict[str, Any] = {}
        if self.use is not None:
            d["use"] = self.use
        if self.family is not None:
            d["family"] = self.family
        if self.given:
            d["given"] = self.given
        if self.prefix:
            d["prefix"] = self.prefix
        if self.suffix:
            d["suffix"] = self.suffix
        if self.text is not None:
            d["text"] = self.text
        return d

    @property
    def display_name(self) -> str:
        if self.text:
            return self.text
        parts: list[str] = []
        if self.prefix:
            parts.extend(self.prefix)
        if self.given:
            parts.extend(self.given)
        if self.family:
            parts.append(self.family)
        return " ".join(parts) if parts else "Unknown"

    def __repr__(self) -> str:
        return f"HumanName({self.display_name!r})"


# ---------------------------------------------------------------------------
# FHIR ContactPoint
# ---------------------------------------------------------------------------

@dataclass
class ContactPoint:
    system: str | None = None   # phone, fax, email, pager, url, sms, other
    value: str | None = None
    use: str | None = None      # home, work, temp, old, mobile
    rank: int | None = None

    @classmethod
    def from_dict(cls, d: dict | None) -> Optional["ContactPoint"]:
        if d is None:
            return None
        return cls(
            system=d.get("system"),
            value=d.get("value"),
            use=d.get("use"),
            rank=d.get("rank"),
        )

    def to_dict(self) -> dict:
        return {k: v for k, v in {
            "system": self.system,
            "value": self.value,
            "use": self.use,
            "rank": self.rank,
        }.items() if v is not None}


# ---------------------------------------------------------------------------
# FHIR Address
# ---------------------------------------------------------------------------

@dataclass
class Address:
    use: str | None = None
    type: str | None = None    # postal, physical, both
    line: list[str] = field(default_factory=list)
    city: str | None = None
    district: str | None = None
    state: str | None = None
    postalCode: str | None = None
    country: str | None = None

    @classmethod
    def from_dict(cls, d: dict | None) -> Optional["Address"]:
        if d is None:
            return None
        line = d.get("line", [])
        return cls(
            use=d.get("use"),
            type=d.get("type"),
            line=line if isinstance(line, list) else [line] if line else [],
            city=d.get("city"),
            district=d.get("district"),
            state=d.get("state"),
            postalCode=d.get("postalCode"),
            country=d.get("country"),
        )

    def to_dict(self) -> dict:
        d: dict[str, Any] = {}
        for attr in ("use", "type", "city", "district", "state", "postalCode", "country"):
            v = getattr(self, attr)
            if v is not None:
                d[attr] = v
        if self.line:
            d["line"] = self.line
        return d


# ---------------------------------------------------------------------------
# FHIR Identifier
# ---------------------------------------------------------------------------

@dataclass
class Identifier:
    use: str | None = None      # usual, official, temp, secondary, old
    system: str | None = None
    value: str | None = None
    type: CodeableConcept | None = None

    @classmethod
    def from_dict(cls, d: dict | None) -> Optional["Identifier"]:
        if d is None:
            return None
        return cls(
            use=d.get("use"),
            system=d.get("system"),
            value=d.get("value"),
            type=CodeableConcept.from_dict(d.get("type")),
        )

    def to_dict(self) -> dict:
        d: dict[str, Any] = {}
        if self.use is not None:
            d["use"] = self.use
        if self.system is not None:
            d["system"] = self.system
        if self.value is not None:
            d["value"] = self.value
        if self.type is not None:
            d["type"] = self.type.to_dict()
        return d


# ---------------------------------------------------------------------------
# FHIR Narrative
# ---------------------------------------------------------------------------

@dataclass
class Narrative:
    status: str | None = None   # generated, extensions, additional, empty
    div: str | None = None      # XHTML

    @classmethod
    def from_dict(cls, d: dict | None) -> Optional["Narrative"]:
        if d is None:
            return None
        return cls(status=d.get("status"), div=d.get("div"))

    def to_dict(self) -> dict:
        d: dict[str, Any] = {}
        if self.status is not None:
            d["status"] = self.status
        if self.div is not None:
            d["div"] = self.div
        return d


# ---------------------------------------------------------------------------
# FHIR Meta
# ---------------------------------------------------------------------------

@dataclass
class Meta:
    versionId: str | None = None
    lastUpdated: str | None = None
    source: str | None = None
    profile: list[str] = field(default_factory=list)

    @classmethod
    def from_dict(cls, d: dict | None) -> Optional["Meta"]:
        if d is None:
            return None
        return cls(
            versionId=d.get("versionId"),
            lastUpdated=d.get("lastUpdated"),
            source=d.get("source"),
            profile=d.get("profile", []),
        )

    def to_dict(self) -> dict:
        d: dict[str, Any] = {}
        if self.versionId is not None:
            d["versionId"] = self.versionId
        if self.lastUpdated is not None:
            d["lastUpdated"] = self.lastUpdated
        if self.source is not None:
            d["source"] = self.source
        if self.profile:
            d["profile"] = self.profile
        return d


# ---------------------------------------------------------------------------
# Common enums
# ---------------------------------------------------------------------------

class ResourceStatus(Enum):
    ACTIVE = "active"
    INACTIVE = "inactive"
    ON_HOLD = "on-hold"
    CANCELLED = "cancelled"
    COMPLETED = "completed"
    ENTERED_IN_ERROR = "entered-in-error"
    DRAFT = "draft"
    UNKNOWN = "unknown"


class EncounterStatus(Enum):
    PLANNED = "planned"
    ARRIVED = "arrived"
    TRIAGED = "triaged"
    IN_PROGRESS = "in-progress"
    ONLEAVE = "onleave"
    FINISHED = "finished"
    CANCELLED = "cancelled"
    ENTERED_IN_ERROR = "entered-in-error"


class ObservationStatus(Enum):
    REGISTERED = "registered"
    PRELIMINARY = "preliminary"
    FINAL = "final"
    AMENDED = "amended"
    CORRECTED = "corrected"
    CANCELLED = "cancelled"
    ENTERED_IN_ERROR = "entered-in-error"


class ConditionClinicalStatus(Enum):
    ACTIVE = "active"
    RECURRENCE = "recurrence"
    RELAPSE = "relapse"
    INACTIVE = "inactive"
    REMISSION = "remission"
    RESOLVED = "resolved"


class ConditionVerificationStatus(Enum):
    CONFIRMED = "confirmed"
    PROVISIONAL = "provisional"
    DIFFERENTIAL = "differential"
    REFUTED = "refuted"
    UNCONFIRMED = "unconfirmed"


class MedicationRequestStatus(Enum):
    ACTIVE = "active"
    ON_HOLD = "on-hold"
    CANCELLED = "cancelled"
    COMPLETED = "completed"
    ENTERED_IN_ERROR = "entered-in-error"
    STOPPED = "stopped"
    DRAFT = "draft"
    UNKNOWN = "unknown"


class ProcedureStatus(Enum):
    PREPARATION = "preparation"
    IN_PROGRESS = "in-progress"
    NOT_DONE = "not-done"
    ON_HOLD = "on-hold"
    STOPPED = "stopped"
    COMPLETED = "completed"
    ENTERED_IN_ERROR = "entered-in-error"
    UNKNOWN = "unknown"


class AllergyIntoleranceClinicalStatus(Enum):
    ACTIVE = "active"
    INACTIVE = "inactive"
    RESOLVED = "resolved"


class AllergyIntoleranceVerificationStatus(Enum):
    CONFIRMED = "confirmed"
    UNCONFIRMED = "unconfirmed"
    REFUTED = "refuted"
    PROVISIONAL = "provisional"


class AllergyIntoleranceType(Enum):
    ALLERGY = "allergy"
    INTOLERANCE = "intolerance"


class AllergyIntoleranceCriticality(Enum):
    LOW = "low"
    HIGH = "high"
    UNABLE_TO_ASSESS = "unable-to-assess"


# ---------------------------------------------------------------------------
# FHIR Resource base
# ---------------------------------------------------------------------------

@dataclass
class FHIRResource:
    """Base class for all FHIR resources."""

    resourceType: str = ""
    id: str | None = None
    meta: Meta | None = None
    text: Narrative | None = None

    def to_dict(self) -> dict:
        d: dict[str, Any] = {"resourceType": self.resourceType}
        if self.id is not None:
            d["id"] = self.id
        if self.meta is not None:
            d["meta"] = self.meta.to_dict()
        if self.text is not None:
            d["text"] = self.text.to_dict()
        return d

    @property
    def full_url(self) -> str:
        """Return a canonical reference string like 'Patient/123'."""
        return f"{self.resourceType}/{self.id}" if self.id else ""


# ---------------------------------------------------------------------------
# Patient
# ---------------------------------------------------------------------------

@dataclass
class Patient(FHIRResource):
    identifier: list[Identifier] = field(default_factory=list)
    active: bool | None = None
    name: list[HumanName] = field(default_factory=list)
    telecom: list[ContactPoint] = field(default_factory=list)
    gender: str | None = None      # male, female, other, unknown
    birthDate: FHIRDate | None = None
    deceasedBoolean: bool | None = None
    deceasedDateTime: FHIRDateTime | None = None
    address: list[Address] = field(default_factory=list)
    maritalStatus: CodeableConcept | None = None
    contact: list[dict] = field(default_factory=list)  # simplified
    communication: list[dict] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: dict) -> "Patient":
        # Handle deceased[x] polymorphism
        deceased_bool = data.get("deceasedBoolean")
        deceased_dt = data.get("deceasedDateTime")

        return cls(
            resourceType=data.get("resourceType", "Patient"),
            id=data.get("id"),
            meta=Meta.from_dict(data.get("meta")),
            text=Narrative.from_dict(data.get("text")),
            identifier=[Identifier.from_dict(i) for i in data.get("identifier", []) if i],
            active=data.get("active"),
            name=[HumanName.from_dict(n) for n in data.get("name", []) if n],
            telecom=[ContactPoint.from_dict(c) for c in data.get("telecom", []) if c],
            gender=data.get("gender"),
            birthDate=FHIRDate.from_value(data.get("birthDate")),
            deceasedBoolean=deceased_bool,
            deceasedDateTime=FHIRDateTime.from_value(deceased_dt),
            address=[Address.from_dict(a) for a in data.get("address", []) if a],
            maritalStatus=CodeableConcept.from_dict(data.get("maritalStatus")),
            contact=data.get("contact", []),
            communication=data.get("communication", []),
        )

    def to_dict(self) -> dict:
        d = super().to_dict()
        if self.identifier:
            d["identifier"] = [i.to_dict() for i in self.identifier]
        if self.active is not None:
            d["active"] = self.active
        if self.name:
            d["name"] = [n.to_dict() for n in self.name]
        if self.telecom:
            d["telecom"] = [c.to_dict() for c in self.telecom]
        if self.gender is not None:
            d["gender"] = self.gender
        if self.birthDate is not None:
            d["birthDate"] = str(self.birthDate)
        if self.deceasedBoolean is not None:
            d["deceasedBoolean"] = self.deceasedBoolean
        elif self.deceasedDateTime is not None:
            d["deceasedDateTime"] = str(self.deceasedDateTime)
        if self.address:
            d["address"] = [a.to_dict() for a in self.address]
        if self.maritalStatus is not None:
            d["maritalStatus"] = self.maritalStatus.to_dict()
        return d

    @property
    def display_name(self) -> str:
        if self.name:
            return self.name[0].display_name
        return "Unknown Patient"

    @property
    def is_deceased(self) -> bool:
        return self.deceasedBoolean is True or self.deceasedDateTime is not None

    def __repr__(self) -> str:
        return f"Patient(id={self.id!r}, name={self.display_name!r})"


# ---------------------------------------------------------------------------
# Encounter
# ---------------------------------------------------------------------------

@dataclass
class Encounter(FHIRResource):
    status: str | None = None
    class_: str | None = None      # FHIR "class" (reserved word)
    type: list[CodeableConcept] = field(default_factory=list)
    serviceType: CodeableConcept | None = None
    priority: CodeableConcept | None = None
    subject: Reference | None = None
    participant: list[dict] = field(default_factory=list)
    period: Period | None = None
    reasonCode: list[CodeableConcept] = field(default_factory=list)
    diagnosis: list[dict] = field(default_factory=list)
    location: list[dict] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: dict) -> "Encounter":
        return cls(
            resourceType=data.get("resourceType", "Encounter"),
            id=data.get("id"),
            meta=Meta.from_dict(data.get("meta")),
            text=Narrative.from_dict(data.get("text")),
            status=data.get("status"),
            class_=data.get("class"),
            type=[CodeableConcept.from_dict(t) for t in data.get("type", []) if t],
            serviceType=CodeableConcept.from_dict(data.get("serviceType")),
            priority=CodeableConcept.from_dict(data.get("priority")),
            subject=Reference.from_dict(data.get("subject")),
            participant=data.get("participant", []),
            period=Period.from_dict(data.get("period")),
            reasonCode=[CodeableConcept.from_dict(r) for r in data.get("reasonCode", []) if r],
            diagnosis=data.get("diagnosis", []),
            location=data.get("location", []),
        )

    def to_dict(self) -> dict:
        d = super().to_dict()
        if self.status is not None:
            d["status"] = self.status
        if self.class_ is not None:
            d["class"] = self.class_
        if self.type:
            d["type"] = [t.to_dict() for t in self.type]
        if self.serviceType is not None:
            d["serviceType"] = self.serviceType.to_dict()
        if self.priority is not None:
            d["priority"] = self.priority.to_dict()
        if self.subject is not None:
            d["subject"] = self.subject.to_dict()
        if self.participant:
            d["participant"] = self.participant
        if self.period is not None:
            d["period"] = self.period.to_dict()
        if self.reasonCode:
            d["reasonCode"] = [r.to_dict() for r in self.reasonCode]
        return d

    @property
    def start_date(self) -> datetime | None:
        if self.period and self.period.start:
            return self.period.start.to_datetime()
        return None

    @property
    def end_date(self) -> datetime | None:
        if self.period and self.period.end:
            return self.period.end.to_datetime()
        return None

    @property
    def display_class(self) -> str:
        if isinstance(self.class_, dict):
            return self.class_.get("display", self.class_.get("code", ""))
        return str(self.class_) if self.class_ else ""

    def __repr__(self) -> str:
        return f"Encounter(id={self.id!r}, status={self.status!r})"


# ---------------------------------------------------------------------------
# Observation
# ---------------------------------------------------------------------------

@dataclass
class Observation(FHIRResource):
    status: str | None = None
    category: list[CodeableConcept] = field(default_factory=list)
    code: CodeableConcept | None = None
    subject: Reference | None = None
    encounter: Reference | None = None
    effectiveDateTime: FHIRDateTime | None = None
    effectivePeriod: Period | None = None
    issued: str | None = None
    valueQuantity: Quantity | None = None
    valueCodeableConcept: CodeableConcept | None = None
    valueString: str | None = None
    valueBoolean: bool | None = None
    valueInteger: int | None = None
    valueDateTime: FHIRDateTime | None = None
    interpretation: list[CodeableConcept] = field(default_factory=list)
    referenceRange: list[dict] = field(default_factory=list)
    component: list["Observation"] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: dict) -> "Observation":
        comp_list = data.get("component", [])
        return cls(
            resourceType=data.get("resourceType", "Observation"),
            id=data.get("id"),
            meta=Meta.from_dict(data.get("meta")),
            text=Narrative.from_dict(data.get("text")),
            status=data.get("status"),
            category=[CodeableConcept.from_dict(c) for c in data.get("category", []) if c],
            code=CodeableConcept.from_dict(data.get("code")),
            subject=Reference.from_dict(data.get("subject")),
            encounter=Reference.from_dict(data.get("encounter")),
            effectiveDateTime=FHIRDateTime.from_value(data.get("effectiveDateTime")),
            effectivePeriod=Period.from_dict(data.get("effectivePeriod")),
            issued=data.get("issued"),
            valueQuantity=Quantity.from_dict(data.get("valueQuantity")),
            valueCodeableConcept=CodeableConcept.from_dict(data.get("valueCodeableConcept")),
            valueString=data.get("valueString"),
            valueBoolean=data.get("valueBoolean"),
            valueInteger=data.get("valueInteger"),
            valueDateTime=FHIRDateTime.from_value(data.get("valueDateTime")),
            interpretation=[CodeableConcept.from_dict(i) for i in data.get("interpretation", []) if i],
            referenceRange=data.get("referenceRange", []),
            component=[cls.from_dict(c) for c in comp_list if c],
        )

    def to_dict(self) -> dict:
        d = super().to_dict()
        if self.status is not None:
            d["status"] = self.status
        if self.category:
            d["category"] = [c.to_dict() for c in self.category]
        if self.code is not None:
            d["code"] = self.code.to_dict()
        if self.subject is not None:
            d["subject"] = self.subject.to_dict()
        if self.encounter is not None:
            d["encounter"] = self.encounter.to_dict()
        if self.effectiveDateTime is not None:
            d["effectiveDateTime"] = str(self.effectiveDateTime)
        elif self.effectivePeriod is not None:
            d["effectivePeriod"] = self.effectivePeriod.to_dict()
        if self.issued is not None:
            d["issued"] = self.issued
        if self.valueQuantity is not None:
            d["valueQuantity"] = self.valueQuantity.to_dict()
        if self.valueCodeableConcept is not None:
            d["valueCodeableConcept"] = self.valueCodeableConcept.to_dict()
        if self.valueString is not None:
            d["valueString"] = self.valueString
        if self.valueBoolean is not None:
            d["valueBoolean"] = self.valueBoolean
        if self.valueInteger is not None:
            d["valueInteger"] = self.valueInteger
        if self.valueDateTime is not None:
            d["valueDateTime"] = str(self.valueDateTime)
        if self.interpretation:
            d["interpretation"] = [i.to_dict() for i in self.interpretation]
        if self.component:
            d["component"] = [c.to_dict() for c in self.component]
        return d

    @property
    def effective_date(self) -> datetime | None:
        if self.effectiveDateTime:
            return self.effectiveDateTime.to_datetime()
        if self.effectivePeriod and self.effectivePeriod.start:
            return self.effectivePeriod.start.to_datetime()
        return None

    @property
    def numeric_value(self) -> float | None:
        """Return the numeric value if this observation has one."""
        if self.valueQuantity and self.valueQuantity.value is not None:
            return self.valueQuantity.value
        if self.valueInteger is not None:
            return float(self.valueInteger)
        return None

    @property
    def display_value(self) -> str:
        if self.valueQuantity is not None:
            v = self.valueQuantity
            unit = v.unit or v.code or ""
            return f"{v.value} {unit}".strip() if v.value is not None else ""
        if self.valueCodeableConcept is not None:
            return self.valueCodeableConcept.first_display or ""
        if self.valueString is not None:
            return self.valueString
        if self.valueBoolean is not None:
            return str(self.valueBoolean)
        if self.valueInteger is not None:
            return str(self.valueInteger)
        if self.valueDateTime is not None:
            return str(self.valueDateTime)
        return ""

    @property
    def display_code(self) -> str:
        if self.code:
            return self.code.first_display or self.code.first_code or ""
        return ""

    def __repr__(self) -> str:
        return f"Observation(id={self.id!r}, code={self.display_code!r})"


# ---------------------------------------------------------------------------
# Condition
# ---------------------------------------------------------------------------

@dataclass
class Condition(FHIRResource):
    clinicalStatus: CodeableConcept | None = None
    verificationStatus: CodeableConcept | None = None
    category: list[CodeableConcept] = field(default_factory=list)
    severity: CodeableConcept | None = None
    code: CodeableConcept | None = None
    bodySite: list[CodeableConcept] = field(default_factory=list)
    subject: Reference | None = None
    encounter: Reference | None = None
    onsetDateTime: FHIRDateTime | None = None
    onsetString: str | None = None
    abatementDateTime: FHIRDateTime | None = None
    recordedDate: str | None = None
    recorder: Reference | None = None
    note: list[dict] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: dict) -> "Condition":
        return cls(
            resourceType=data.get("resourceType", "Condition"),
            id=data.get("id"),
            meta=Meta.from_dict(data.get("meta")),
            text=Narrative.from_dict(data.get("text")),
            clinicalStatus=CodeableConcept.from_dict(data.get("clinicalStatus")),
            verificationStatus=CodeableConcept.from_dict(data.get("verificationStatus")),
            category=[CodeableConcept.from_dict(c) for c in data.get("category", []) if c],
            severity=CodeableConcept.from_dict(data.get("severity")),
            code=CodeableConcept.from_dict(data.get("code")),
            bodySite=[CodeableConcept.from_dict(b) for b in data.get("bodySite", []) if b],
            subject=Reference.from_dict(data.get("subject")),
            encounter=Reference.from_dict(data.get("encounter")),
            onsetDateTime=FHIRDateTime.from_value(data.get("onsetDateTime")),
            onsetString=data.get("onsetString"),
            abatementDateTime=FHIRDateTime.from_value(data.get("abatementDateTime")),
            recordedDate=data.get("recordedDate"),
            recorder=Reference.from_dict(data.get("recorder")),
            note=data.get("note", []),
        )

    def to_dict(self) -> dict:
        d = super().to_dict()
        if self.clinicalStatus is not None:
            d["clinicalStatus"] = self.clinicalStatus.to_dict()
        if self.verificationStatus is not None:
            d["verificationStatus"] = self.verificationStatus.to_dict()
        if self.category:
            d["category"] = [c.to_dict() for c in self.category]
        if self.severity is not None:
            d["severity"] = self.severity.to_dict()
        if self.code is not None:
            d["code"] = self.code.to_dict()
        if self.bodySite:
            d["bodySite"] = [b.to_dict() for b in self.bodySite]
        if self.subject is not None:
            d["subject"] = self.subject.to_dict()
        if self.encounter is not None:
            d["encounter"] = self.encounter.to_dict()
        if self.onsetDateTime is not None:
            d["onsetDateTime"] = str(self.onsetDateTime)
        if self.onsetString is not None:
            d["onsetString"] = self.onsetString
        if self.abatementDateTime is not None:
            d["abatementDateTime"] = str(self.abatementDateTime)
        if self.recordedDate is not None:
            d["recordedDate"] = self.recordedDate
        return d

    @property
    def is_active(self) -> bool:
        if self.clinicalStatus:
            code = self.clinicalStatus.first_code
            return code in ("active", "recurrence", "relapse")
        return False

    @property
    def onset_date(self) -> datetime | None:
        if self.onsetDateTime:
            return self.onsetDateTime.to_datetime()
        return None

    @property
    def display_code(self) -> str:
        if self.code:
            return self.code.first_display or self.code.first_code or ""
        return ""

    def __repr__(self) -> str:
        return f"Condition(id={self.id!r}, code={self.display_code!r})"


# ---------------------------------------------------------------------------
# MedicationRequest
# ---------------------------------------------------------------------------

@dataclass
class MedicationRequest(FHIRResource):
    status: str | None = None
    intent: str | None = None
    medicationCodeableConcept: CodeableConcept | None = None
    medicationReference: Reference | None = None
    subject: Reference | None = None
    encounter: Reference | None = None
    authoredOn: FHIRDateTime | None = None
    requester: Reference | None = None
    dosageInstruction: list[dict] = field(default_factory=list)
    dispenseRequest: dict | None = None
    note: list[dict] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: dict) -> "MedicationRequest":
        return cls(
            resourceType=data.get("resourceType", "MedicationRequest"),
            id=data.get("id"),
            meta=Meta.from_dict(data.get("meta")),
            text=Narrative.from_dict(data.get("text")),
            status=data.get("status"),
            intent=data.get("intent"),
            medicationCodeableConcept=CodeableConcept.from_dict(
                data.get("medicationCodeableConcept")
            ),
            medicationReference=Reference.from_dict(data.get("medicationReference")),
            subject=Reference.from_dict(data.get("subject")),
            encounter=Reference.from_dict(data.get("encounter")),
            authoredOn=FHIRDateTime.from_value(data.get("authoredOn")),
            requester=Reference.from_dict(data.get("requester")),
            dosageInstruction=data.get("dosageInstruction", []),
            dispenseRequest=data.get("dispenseRequest"),
            note=data.get("note", []),
        )

    def to_dict(self) -> dict:
        d = super().to_dict()
        if self.status is not None:
            d["status"] = self.status
        if self.intent is not None:
            d["intent"] = self.intent
        if self.medicationCodeableConcept is not None:
            d["medicationCodeableConcept"] = self.medicationCodeableConcept.to_dict()
        if self.medicationReference is not None:
            d["medicationReference"] = self.medicationReference.to_dict()
        if self.subject is not None:
            d["subject"] = self.subject.to_dict()
        if self.encounter is not None:
            d["encounter"] = self.encounter.to_dict()
        if self.authoredOn is not None:
            d["authoredOn"] = str(self.authoredOn)
        if self.dosageInstruction:
            d["dosageInstruction"] = self.dosageInstruction
        return d

    @property
    def display_medication(self) -> str:
        if self.medicationCodeableConcept:
            return self.medicationCodeableConcept.first_display or self.medicationCodeableConcept.first_code or ""
        if self.medicationReference:
            return self.medicationReference.display or str(self.medicationReference)
        return ""

    @property
    def is_active(self) -> bool:
        return self.status in ("active",)

    @property
    def authored_date(self) -> datetime | None:
        if self.authoredOn:
            return self.authoredOn.to_datetime()
        return None

    @property
    def dosage_text(self) -> str:
        """Return a human-readable dosage string."""
        if not self.dosageInstruction:
            return ""
        first = self.dosageInstruction[0]
        parts: list[str] = []
        text = first.get("text")
        if text:
            parts.append(text)
        else:
            for timing in first.get("timing", []):
                if isinstance(timing, dict):
                    code = timing.get("code", {})
                    if isinstance(code, dict):
                        parts.append(code.get("text", ""))
            dose = first.get("doseAndRate", [])
            if dose and isinstance(dose, list):
                d = dose[0]
                if isinstance(d, dict):
                    qty = d.get("doseQuantity", {})
                    if isinstance(qty, dict):
                        val = qty.get("value", "")
                        unit = qty.get("unit", "")
                        parts.append(f"{val} {unit}".strip())
        return " ".join(p for p in parts if p)

    def __repr__(self) -> str:
        return f"MedicationRequest(id={self.id!r}, med={self.display_medication!r})"


# ---------------------------------------------------------------------------
# Procedure
# ---------------------------------------------------------------------------

@dataclass
class Procedure(FHIRResource):
    status: str | None = None
    code: CodeableConcept | None = None
    subject: Reference | None = None
    encounter: Reference | None = None
    performedDateTime: FHIRDateTime | None = None
    performedPeriod: Period | None = None
    performer: list[dict] = field(default_factory=list)
    reasonCode: list[CodeableConcept] = field(default_factory=list)
    bodySite: list[CodeableConcept] = field(default_factory=list)
    outcome: CodeableConcept | None = None
    note: list[dict] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: dict) -> "Procedure":
        return cls(
            resourceType=data.get("resourceType", "Procedure"),
            id=data.get("id"),
            meta=Meta.from_dict(data.get("meta")),
            text=Narrative.from_dict(data.get("text")),
            status=data.get("status"),
            code=CodeableConcept.from_dict(data.get("code")),
            subject=Reference.from_dict(data.get("subject")),
            encounter=Reference.from_dict(data.get("encounter")),
            performedDateTime=FHIRDateTime.from_value(data.get("performedDateTime")),
            performedPeriod=Period.from_dict(data.get("performedPeriod")),
            performer=data.get("performer", []),
            reasonCode=[CodeableConcept.from_dict(r) for r in data.get("reasonCode", []) if r],
            bodySite=[CodeableConcept.from_dict(b) for b in data.get("bodySite", []) if b],
            outcome=CodeableConcept.from_dict(data.get("outcome")),
            note=data.get("note", []),
        )

    def to_dict(self) -> dict:
        d = super().to_dict()
        if self.status is not None:
            d["status"] = self.status
        if self.code is not None:
            d["code"] = self.code.to_dict()
        if self.subject is not None:
            d["subject"] = self.subject.to_dict()
        if self.encounter is not None:
            d["encounter"] = self.encounter.to_dict()
        if self.performedDateTime is not None:
            d["performedDateTime"] = str(self.performedDateTime)
        elif self.performedPeriod is not None:
            d["performedPeriod"] = self.performedPeriod.to_dict()
        if self.performer:
            d["performer"] = self.performer
        if self.reasonCode:
            d["reasonCode"] = [r.to_dict() for r in self.reasonCode]
        return d

    @property
    def performed_date(self) -> datetime | None:
        if self.performedDateTime:
            return self.performedDateTime.to_datetime()
        if self.performedPeriod and self.performedPeriod.start:
            return self.performedPeriod.start.to_datetime()
        return None

    @property
    def display_code(self) -> str:
        if self.code:
            return self.code.first_display or self.code.first_code or ""
        return ""

    def __repr__(self) -> str:
        return f"Procedure(id={self.id!r}, code={self.display_code!r})"


# ---------------------------------------------------------------------------
# AllergyIntolerance
# ---------------------------------------------------------------------------

@dataclass
class AllergyIntolerance(FHIRResource):
    clinicalStatus: CodeableConcept | None = None
    verificationStatus: CodeableConcept | None = None
    type: CodeableConcept | None = None
    category: list[str] = field(default_factory=list)
    criticality: str | None = None
    code: CodeableConcept | None = None
    patient: Reference | None = None
    onsetDateTime: FHIRDateTime | None = None
    recordedDate: str | None = None
    recorder: Reference | None = None
    reaction: list[dict] = field(default_factory=list)
    note: list[dict] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: dict) -> "AllergyIntolerance":
        return cls(
            resourceType=data.get("resourceType", "AllergyIntolerance"),
            id=data.get("id"),
            meta=Meta.from_dict(data.get("meta")),
            text=Narrative.from_dict(data.get("text")),
            clinicalStatus=CodeableConcept.from_dict(data.get("clinicalStatus")),
            verificationStatus=CodeableConcept.from_dict(data.get("verificationStatus")),
            type=CodeableConcept.from_dict(data.get("type")),
            category=data.get("category", []),
            criticality=data.get("criticality"),
            code=CodeableConcept.from_dict(data.get("code")),
            patient=Reference.from_dict(data.get("patient")),
            onsetDateTime=FHIRDateTime.from_value(data.get("onsetDateTime")),
            recordedDate=data.get("recordedDate"),
            recorder=Reference.from_dict(data.get("recorder")),
            reaction=data.get("reaction", []),
            note=data.get("note", []),
        )

    def to_dict(self) -> dict:
        d = super().to_dict()
        if self.clinicalStatus is not None:
            d["clinicalStatus"] = self.clinicalStatus.to_dict()
        if self.verificationStatus is not None:
            d["verificationStatus"] = self.verificationStatus.to_dict()
        if self.type is not None:
            d["type"] = self.type.to_dict()
        if self.category:
            d["category"] = self.category
        if self.criticality is not None:
            d["criticality"] = self.criticality
        if self.code is not None:
            d["code"] = self.code.to_dict()
        if self.patient is not None:
            d["patient"] = self.patient.to_dict()
        if self.onsetDateTime is not None:
            d["onsetDateTime"] = str(self.onsetDateTime)
        if self.recordedDate is not None:
            d["recordedDate"] = self.recordedDate
        return d

    @property
    def is_active(self) -> bool:
        if self.clinicalStatus:
            code = self.clinicalStatus.first_code
            return code == "active"
        return False

    @property
    def display_code(self) -> str:
        if self.code:
            return self.code.first_display or self.code.first_code or ""
        return ""

    def __repr__(self) -> str:
        return f"AllergyIntolerance(id={self.id!r}, code={self.display_code!r})"


# ---------------------------------------------------------------------------
# Resource type registry
# ---------------------------------------------------------------------------

RESOURCE_TYPES: dict[str, Type[FHIRResource]] = {
    "Patient": Patient,
    "Encounter": Encounter,
    "Observation": Observation,
    "Condition": Condition,
    "MedicationRequest": MedicationRequest,
    "Procedure": Procedure,
    "AllergyIntolerance": AllergyIntolerance,
}


def parse_resource(data: dict) -> FHIRResource:
    """Parse a FHIR resource dict into its typed model.

    Raises ValueError if the resourceType is not supported.
    """
    resource_type = data.get("resourceType")
    if not resource_type:
        raise ValueError("FHIR resource missing 'resourceType' field")
    cls = RESOURCE_TYPES.get(resource_type)
    if cls is None:
        raise ValueError(f"Unsupported FHIR resource type: {resource_type}")
    return cls.from_dict(data)


def serialize_resource(resource: FHIRResource) -> dict:
    """Serialize a typed FHIR resource back to a dict."""
    return resource.to_dict()
