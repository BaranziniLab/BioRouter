"""
FHIR Parser CLI.

Loads a FHIR Bundle JSON file and prints a patient summary + timeline.

Usage:
    fhir-parser <bundle.json>
    python -m fhir_parser <bundle.json>
    python -m fhir_parser.cli <bundle.json>

Options:
    --json          Output in JSON instead of formatted text
    --timeline-only Print only the timeline
    --summary-only  Print only the patient summary
"""

from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime
from typing import TextIO

from .bundle import parse_bundle, BundleFHIR
from .timeline import build_timeline, PatientTimeline, EventType
from .query import (
    query_active_conditions,
    query_latest_vitals,
    query_medications_on_date,
    query_observation_trends,
)
from .validate import validate_bundle, ValidationResult


def _print_header(title: str, file: TextIO | None = None) -> None:
    if file is None:
        file = sys.stdout
    file.write("\n" + "=" * 60 + "\n")
    file.write(f"  {title}\n")
    file.write("=" * 60 + "\n")


def _print_section(title: str, file: TextIO | None = None) -> None:
    if file is None:
        file = sys.stdout
    file.write(f"\n--- {title} ---\n")


def print_patient_summary(bundle: BundleFHIR, file: TextIO | None = None) -> None:
    """Print a formatted patient summary."""
    if file is None:
        file = sys.stdout
    patient = bundle.get_patient()
    if patient is None:
        file.write("No patient found in bundle.\n")
        return

    _print_header("PATIENT SUMMARY", file)

    # Demographics
    _print_section("Demographics", file)
    file.write(f"  Name:       {patient.display_name}\n")
    file.write(f"  Gender:     {patient.gender or 'Unknown'}\n")
    file.write(f"  Birth Date: {patient.birthDate or 'Unknown'}\n")
    if patient.is_deceased:
        file.write(f"  Deceased:   Yes\n")

    # Identifiers
    if patient.identifier:
        _print_section("Identifiers", file)
        for ident in patient.identifier:
            file.write(f"  {ident.system}: {ident.value}\n")

    # Contact
    if patient.telecom:
        _print_section("Contact", file)
        for tp in patient.telecom:
            file.write(f"  {tp.system}: {tp.value} ({tp.use or 'unknown use'})\n")

    # Address
    if patient.address:
        _print_section("Address", file)
        for addr in patient.address:
            line = ", ".join(addr.line) if addr.line else ""
            city_state = f"{addr.city}, {addr.state} {addr.postalCode}".strip()
            parts = [p for p in [line, city_state, addr.country] if p]
            file.write(f"  {', '.join(parts)}\n")

    # Resource counts
    _print_section("Bundle Contents", file)
    counts = bundle.resource_type_counts
    for rtype, count in sorted(counts.items()):
        file.write(f"  {rtype}: {count}\n")
    file.write(f"  Total resources: {bundle.total_resources}\n")

    # Active conditions
    conditions = query_active_conditions(bundle)
    if conditions:
        _print_section("Active Conditions", file)
        for c in conditions:
            onset = c.onset_date.strftime("%Y-%m-%d") if c.onset_date else "Unknown"
            file.write(f"  • {c.code_display} (since {onset})\n")
            if c.severity:
                file.write(f"    Severity: {c.severity}\n")

    # Latest vitals
    vitals = query_latest_vitals(bundle)
    if vitals:
        _print_section("Latest Vitals", file)
        for v in vitals:
            date_str = v.effective_date.strftime("%Y-%m-%d %H:%M") if v.effective_date else "Unknown"
            file.write(f"  {v.code_display}: {v.value}  ({date_str})\n")

    # Medications
    meds = query_medications_on_date(bundle, datetime.now().date())
    if meds:
        _print_section("Current Medications", file)
        for m in meds:
            file.write(f"  • {m.medication_display}")
            if m.dosage:
                file.write(f" — {m.dosage}")
            file.write("\n")


def print_timeline(timeline: PatientTimeline, file: TextIO | None = None) -> None:
    """Print the patient timeline."""
    if file is None:
        file = sys.stdout
    _print_header("PATIENT TIMELINE", file)

    date_start, date_end = timeline.date_range
    if date_start and date_end:
        file.write(f"  Period: {date_start.strftime('%Y-%m-%d')} to {date_end.strftime('%Y-%m-%d')}\n")
    file.write(f"  Total events: {len(timeline.events)}\n")

    counts = timeline.event_type_counts
    for etype, count in sorted(counts.items()):
        file.write(f"    {etype}: {count}\n")

    file.write("\n")
    file.write(f"  {'Date':<22} {'Type':<14} {'Event'}\n")
    file.write(f"  {'-'*22} {'-'*14} {'-'*40}\n")

    for event in timeline:
        ts = event.timestamp.strftime("%Y-%m-%d %H:%M") if event.timestamp else "N/A"
        file.write(f"  {ts:<22} {event.event_type.value:<14} {event.display}\n")


def print_validation(result: ValidationResult, file: TextIO | None = None) -> None:
    """Print validation results."""
    if file is None:
        file = sys.stdout
    _print_header("VALIDATION", file)
    file.write(f"  {result}\n")
    if result.errors:
        _print_section("Issues", file)
        for err in result.errors:
            file.write(f"  {err}\n")


def format_json(bundle: BundleFHIR, timeline: PatientTimeline) -> dict:
    """Format bundle and timeline as a JSON-serialisable dict."""
    conditions = query_active_conditions(bundle)
    vitals = query_latest_vitals(bundle)
    validation = validate_bundle(bundle)

    patient = bundle.get_patient()
    return {
        "patient": {
            "id": patient.id if patient else None,
            "name": patient.display_name if patient else None,
            "gender": patient.gender if patient else None,
            "birthDate": str(patient.birthDate) if patient and patient.birthDate else None,
        },
        "bundle_summary": {
            "type": bundle.type,
            "total_resources": bundle.total_resources,
            "resource_type_counts": bundle.resource_type_counts,
        },
        "active_conditions": [
            {
                "code": c.code_display,
                "status": c.clinical_status,
                "onset": c.onset_date.isoformat() if c.onset_date else None,
            }
            for c in conditions
        ],
        "latest_vitals": [
            {
                "code": v.code_display,
                "value": v.value,
                "unit": v.unit,
                "date": v.effective_date.isoformat() if v.effective_date else None,
            }
            for v in vitals
        ],
        "timeline": {
            "total_events": len(timeline.events),
            "event_type_counts": timeline.event_type_counts,
            "events": [
                {
                    "type": e.event_type.value,
                    "timestamp": e.timestamp.isoformat() if e.timestamp else None,
                    "display": e.display,
                }
                for e in timeline
            ],
        },
        "validation": {
            "is_valid": validation.is_valid,
            "error_count": validation.error_count,
            "warning_count": validation.warning_count,
        },
    }


def main(argv: list[str] | None = None) -> int:
    """CLI entry point."""
    parser = argparse.ArgumentParser(
        prog="fhir-parser",
        description="FHIR R4 Bundle Parser & Patient Timeline Toolkit",
    )
    parser.add_argument(
        "bundle_file",
        nargs="?",
        help="Path to a FHIR Bundle JSON file (or - for stdin)",
    )
    parser.add_argument(
        "--json", dest="output_json", action="store_true",
        help="Output in JSON format",
    )
    parser.add_argument(
        "--timeline-only", action="store_true",
        help="Print only the timeline",
    )
    parser.add_argument(
        "--summary-only", action="store_true",
        help="Print only the patient summary",
    )
    parser.add_argument(
        "--validate-only", action="store_true",
        help="Run validation and print results only",
    )

    args = parser.parse_args(argv)

    # Load bundle
    if args.bundle_file is None or args.bundle_file == "-":
        raw = sys.stdin.read()
    else:
        try:
            with open(args.bundle_file, "r", encoding="utf-8") as f:
                raw = f.read()
        except FileNotFoundError:
            print(f"Error: File not found: {args.bundle_file}", file=sys.stderr)
            return 1
        except OSError as e:
            print(f"Error reading file: {e}", file=sys.stderr)
            return 1

    try:
        bundle = parse_bundle(raw)
    except Exception as e:
        print(f"Error parsing FHIR bundle: {e}", file=sys.stderr)
        return 1

    # Build timeline
    timeline = build_timeline(bundle)

    # JSON output
    if args.output_json:
        data = format_json(bundle, timeline)
        print(json.dumps(data, indent=2, default=str))
        return 0

    # Text output
    if args.validate_only:
        result = validate_bundle(bundle)
        print_validation(result)
        return 0 if result.is_valid else 1

    if not args.timeline_only:
        print_patient_summary(bundle)

    if not args.summary_only:
        print_timeline(timeline)

    if not args.summary_only and not args.timeline_only:
        result = validate_bundle(bundle)
        print_validation(result)

    return 0


if __name__ == "__main__":
    sys.exit(main())
