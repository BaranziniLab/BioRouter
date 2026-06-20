# FHIR Parser & Patient Timeline Toolkit

A pure-Python FHIR R4 parser, patient-timeline builder, and query engine.

## Features

- **FHIR R4 Resource Parsing** – Patient, Encounter, Observation, Condition, MedicationRequest, Procedure, AllergyIntolerance from JSON (single resources and Bundles)
- **Typed In-Memory Model** – Dataclass-based resource representations with proper type hints
- **Reference Resolution** – Automatic resolution of internal FHIR references within Bundles
- **Patient Timeline Builder** – Merges encounters, observations, conditions into a chronological event stream
- **Query Engine** – Active conditions, latest vitals, medications on a date, observation trends
- **FHIR Validation** – Required fields, value sets, reference integrity with helpful error messages
- **CLI** – Load a bundle and print a patient summary + timeline

## Project Structure

```
src/fhir_parser/
├── __init__.py      # Package init, version
├── resources.py     # Typed FHIR resource models (Patient, Encounter, etc.)
├── bundle.py        # Bundle parsing and reference resolution
├── timeline.py      # Patient timeline builder
├── query.py         # Query engine (conditions, vitals, medications, trends)
├── validate.py      # FHIR validation with helpful errors
├── cli.py           # Command-line interface
└── synthetic.py     # Synthetic FHIR bundle generator for testing
tests/
├── test_resources.py
├── test_bundle.py
├── test_timeline.py
├── test_query.py
├── test_validate.py
├── test_cli.py
└── test_roundtrip.py
```

## Installation

```bash
pip install -e ".[dev]"
```

## Usage

```bash
# Print patient summary and timeline from a FHIR bundle
fhir-parser path/to/bundle.json

# Or use as a library
from fhir_parser.bundle import parse_bundle
from fhir_parser.timeline import build_timeline
from fhir_parser.query import query_active_conditions

bundle = parse_bundle(open("bundle.json").read())
timeline = build_timeline(bundle)
```

## Running Tests

```bash
pytest
```

## FHIR R4 Resources Supported

| Resource               | Status |
|------------------------|--------|
| Patient                | ✅      |
| Encounter              | ✅      |
| Observation            | ✅      |
| Condition              | ✅      |
| MedicationRequest      | ✅      |
| Procedure              | ✅      |
| AllergyIntolerance     | ✅      |

## License

MIT
