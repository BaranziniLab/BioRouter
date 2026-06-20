"""
FHIR Bundle parsing and reference resolution.

Supports:
  - Parsing Bundle JSON into a typed BundleFHIR object
  - Resolving internal references (fullUrl + resource.id) within a bundle
  - Extracting resources by type
  - Iterating entries in order
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from typing import Any, Optional, Type

from .resources import (
    FHIRResource,
    Patient,
    parse_resource,
    serialize_resource,
    RESOURCE_TYPES,
)


@dataclass
class BundleEntry:
    """A single entry in a FHIR Bundle."""

    fullUrl: str | None = None
    resource: FHIRResource | None = None
    search: dict | None = None
    request: dict | None = None
    response: dict | None = None

    @classmethod
    def from_dict(cls, data: dict) -> "BundleEntry":
        resource_data = data.get("resource")
        resource = None
        if resource_data:
            try:
                resource = parse_resource(resource_data)
            except (ValueError, KeyError):
                resource = None
        return cls(
            fullUrl=data.get("fullUrl"),
            resource=resource,
            search=data.get("search"),
            request=data.get("request"),
            response=data.get("response"),
        )

    def to_dict(self) -> dict:
        d: dict[str, Any] = {}
        if self.fullUrl is not None:
            d["fullUrl"] = self.fullUrl
        if self.resource is not None:
            d["resource"] = serialize_resource(self.resource)
        if self.search is not None:
            d["search"] = self.search
        if self.request is not None:
            d["request"] = self.request
        if self.response is not None:
            d["response"] = self.response
        return d

    @property
    def resource_type(self) -> str | None:
        if self.resource:
            return self.resource.resourceType
        if self.fullUrl and "/" in self.fullUrl:
            return self.fullUrl.split("/")[0]
        return None

    @property
    def resource_id(self) -> str | None:
        if self.resource and self.resource.id:
            return self.resource.id
        if self.fullUrl and "/" in self.fullUrl:
            return self.fullUrl.split("/", 1)[1]
        return None

    def __repr__(self) -> str:
        return f"BundleEntry(type={self.resource_type!r}, id={self.resource_id!r})"


@dataclass
class BundleFHIR:
    """A FHIR Bundle (collection, searchset, transaction, etc.)."""

    resourceType: str = "Bundle"
    id: str | None = None
    meta: dict | None = None
    type: str | None = None
    total: int | None = None
    link: list[dict] = field(default_factory=list)
    entry: list[BundleEntry] = field(default_factory=list)

    _ref_index: dict[str, FHIRResource] = field(default_factory=dict, repr=False)

    @classmethod
    def from_dict(cls, data: dict) -> "BundleFHIR":
        """Parse a FHIR Bundle from a dict."""
        entries = [BundleEntry.from_dict(e) for e in data.get("entry", [])]
        bundle = cls(
            resourceType=data.get("resourceType", "Bundle"),
            id=data.get("id"),
            meta=data.get("meta"),
            type=data.get("type"),
            total=data.get("total"),
            link=data.get("link", []),
            entry=entries,
        )
        bundle._build_ref_index()
        return bundle

    @classmethod
    def from_json(cls, json_str: str) -> "BundleFHIR":
        """Parse a FHIR Bundle from a JSON string."""
        data = json.loads(json_str)
        if isinstance(data, list):
            data = {
                "resourceType": "Bundle",
                "type": "collection",
                "entry": [
                    {"fullUrl": f"{r.get('resourceType', 'Unknown')}/{r.get('id', '')}", "resource": r}
                    for r in data
                ],
            }
        return cls.from_dict(data)

    @classmethod
    def from_resource_list(cls, resources: list[FHIRResource]) -> "BundleFHIR":
        """Create a Bundle from a list of already-parsed resources."""
        entries = []
        for r in resources:
            entries.append(BundleEntry(
                fullUrl=f"{r.resourceType}/{r.id}",
                resource=r,
            ))
        bundle = cls(type="collection", entry=entries)
        bundle._build_ref_index()
        return bundle

    def to_dict(self) -> dict:
        d: dict[str, Any] = {"resourceType": self.resourceType}
        if self.id is not None:
            d["id"] = self.id
        if self.meta is not None:
            d["meta"] = self.meta
        if self.type is not None:
            d["type"] = self.type
        if self.total is not None:
            d["total"] = self.total
        if self.link:
            d["link"] = self.link
        if self.entry:
            d["entry"] = [e.to_dict() for e in self.entry]
        return d

    def to_json(self, indent: int = 2) -> str:
        return json.dumps(self.to_dict(), indent=indent, default=str)

    def _build_ref_index(self) -> None:
        """Build an index of 'Type/Id' -> resource for reference resolution."""
        self._ref_index.clear()
        for entry in self.entry:
            if entry.resource is not None:
                rid = entry.resource.id
                rtype = entry.resource.resourceType
                if rid:
                    key = f"{rtype}/{rid}"
                    self._ref_index[key] = entry.resource
                if entry.fullUrl:
                    self._ref_index[entry.fullUrl] = entry.resource

    def resolve_reference(self, ref_str: str) -> FHIRResource | None:
        """Resolve a reference string like 'Patient/123' to its resource."""
        if not ref_str:
            return None
        return self._ref_index.get(ref_str)

    def get_entries_by_type(self, resource_type: str) -> list[BundleEntry]:
        return [e for e in self.entry if e.resource_type == resource_type]

    def get_resources_by_type(self, resource_type: str) -> list[FHIRResource]:
        return [e.resource for e in self.entry
                if e.resource is not None and e.resource.resourceType == resource_type]

    def get_patient(self) -> Patient | None:
        for e in self.entry:
            if isinstance(e.resource, Patient):
                return e.resource
        return None

    @property
    def resources(self) -> list[FHIRResource]:
        return [e.resource for e in self.entry if e.resource is not None]

    @property
    def patient_count(self) -> int:
        return len(self.get_entries_by_type("Patient"))

    @property
    def total_resources(self) -> int:
        return len(self.resources)

    @property
    def resource_type_counts(self) -> dict[str, int]:
        counts: dict[str, int] = {}
        for e in self.entry:
            rt = e.resource_type
            if rt:
                counts[rt] = counts.get(rt, 0) + 1
        return counts

    def __iter__(self):
        return iter(self.entry)

    def __len__(self):
        return len(self.entry)

    def __repr__(self) -> str:
        return (
            f"BundleFHIR(type={self.type!r}, "
            f"entries={len(self.entry)}, "
            f"types={self.resource_type_counts})"
        )


def parse_bundle(data: str | dict) -> BundleFHIR:
    """Convenience function to parse a bundle from JSON string or dict."""
    if isinstance(data, str):
        return BundleFHIR.from_json(data)
    return BundleFHIR.from_dict(data)


def merge_bundles(*bundles: BundleFHIR) -> BundleFHIR:
    """Merge multiple bundles into one, deduplicating resources by id."""
    seen: set[str] = set()
    all_entries: list[BundleEntry] = []

    for bundle in bundles:
        for entry in bundle:
            if entry.resource is None:
                continue
            rid = f"{entry.resource.resourceType}/{entry.resource.id}"
            if rid not in seen:
                seen.add(rid)
                all_entries.append(entry)

    merged = BundleFHIR(type="collection", entry=all_entries)
    merged._build_ref_index()
    return merged
