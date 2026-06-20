"""
Tests for cli.py — CLI invocation via python -m and direct call.
"""

import json
import os
import tempfile
import pytest

from fhir_parser.cli import main, print_patient_summary, print_timeline, print_validation
from fhir_parser.bundle import parse_bundle, BundleFHIR
from fhir_parser.timeline import build_timeline
from fhir_parser.synthetic import (
    generate_patient_bundle,
    generate_simple_bundle,
    generate_empty_bundle,
)


@pytest.fixture
def bundle_file(tmp_path):
    """Write a patient bundle to a temp file and return its path."""
    raw = generate_patient_bundle()
    path = tmp_path / "test_bundle.json"
    path.write_text(json.dumps(raw))
    return str(path)


@pytest.fixture
def simple_bundle_file(tmp_path):
    """Write a simple bundle to a temp file."""
    raw = generate_simple_bundle()
    path = tmp_path / "simple.json"
    path.write_text(json.dumps(raw))
    return str(path)


@pytest.fixture
def malformed_bundle_file(tmp_path):
    """Write a malformed bundle to a temp file."""
    from fhir_parser.synthetic import generate_malformed_bundle
    raw = generate_malformed_bundle()
    path = tmp_path / "malformed.json"
    path.write_text(json.dumps(raw))
    return str(path)


@pytest.fixture
def empty_bundle_file(tmp_path):
    """Write an empty bundle to a temp file."""
    raw = generate_empty_bundle()
    path = tmp_path / "empty.json"
    path.write_text(json.dumps(raw))
    return str(path)


class TestCLIMain:
    def test_print_summary(self, bundle_file):
        """main() should return 0 and print output."""
        ret = main([bundle_file])
        assert ret == 0

    def test_json_output(self, bundle_file, capsys):
        """--json should output valid JSON."""
        ret = main([bundle_file, "--json"])
        assert ret == 0
        captured = capsys.readouterr()
        data = json.loads(captured.out)
        assert "patient" in data
        assert "timeline" in data
        assert "active_conditions" in data
        assert "latest_vitals" in data
        assert "validation" in data

    def test_timeline_only(self, bundle_file, capsys):
        """--timeline-only should show only timeline."""
        ret = main([bundle_file, "--timeline-only"])
        assert ret == 0
        captured = capsys.readouterr()
        assert "TIMELINE" in captured.out
        assert "PATIENT SUMMARY" not in captured.out

    def test_summary_only(self, bundle_file, capsys):
        """--summary-only should show only summary."""
        ret = main([bundle_file, "--summary-only"])
        assert ret == 0
        captured = capsys.readouterr()
        assert "PATIENT SUMMARY" in captured.out

    def test_validate_only_valid(self, simple_bundle_file, capsys):
        """--validate-only with valid bundle returns 0."""
        ret = main([simple_bundle_file, "--validate-only"])
        assert ret == 0
        captured = capsys.readouterr()
        assert "VALIDATION" in captured.out

    def test_validate_only_invalid(self, malformed_bundle_file, capsys):
        """--validate-only with malformed bundle returns 1."""
        ret = main([malformed_bundle_file, "--validate-only"])
        assert ret == 1

    def test_missing_file(self, capsys):
        """Non-existent file should return 1."""
        ret = main(["/nonexistent/path.json"])
        assert ret == 1

    def test_invalid_json(self, tmp_path, capsys):
        """Invalid JSON should return 1."""
        path = tmp_path / "bad.json"
        path.write_text("not json at all {{{")
        ret = main([str(path)])
        assert ret == 1

    def test_stdin_mode(self, monkeypatch, capsys):
        """Reading from stdin should work with -."""
        raw = generate_simple_bundle()
        monkeypatch.setattr("sys.stdin", __import__("io").StringIO(json.dumps(raw)))
        ret = main(["-"])
        assert ret == 0

    def test_empty_bundle(self, empty_bundle_file, capsys):
        """Empty bundle should work without errors."""
        ret = main([empty_bundle_file])
        assert ret == 0
        captured = capsys.readouterr()
        assert "No patient" in captured.out
        assert "TIMELINE" in captured.out


class TestCLIDirectFunctions:
    def test_print_patient_summary(self, capsys):
        raw = generate_patient_bundle()
        bundle = parse_bundle(raw)
        print_patient_summary(bundle)
        captured = capsys.readouterr()
        assert "Jane" in captured.out
        assert "Demographics" in captured.out
        assert "Active Conditions" in captured.out
        assert "Latest Vitals" in captured.out
        assert "Current Medications" in captured.out

    def test_print_timeline(self, capsys):
        raw = generate_patient_bundle()
        bundle = parse_bundle(raw)
        timeline = build_timeline(bundle)
        print_timeline(timeline)
        captured = capsys.readouterr()
        assert "TIMELINE" in captured.out
        assert "encounter" in captured.out
        assert "observation" in captured.out

    def test_print_validation(self, capsys):
        raw = generate_patient_bundle()
        bundle = parse_bundle(raw)
        from fhir_parser.validate import validate_bundle
        result = validate_bundle(bundle)
        print_validation(result)
        captured = capsys.readouterr()
        assert "VALIDATION" in captured.out

    def test_print_summary_no_patient(self, capsys):
        raw = {
            "resourceType": "Bundle",
            "type": "collection",
            "entry": [{"resource": {"resourceType": "Observation", "id": "o1", "status": "final", "code": {"text": "x"}}}],
        }
        bundle = parse_bundle(raw)
        print_patient_summary(bundle)
        captured = capsys.readouterr()
        assert "No patient" in captured.out


class TestCLIAsModule:
    def test_python_m_module(self, bundle_file):
        """python -m fhir_parser should work."""
        import subprocess, os
        src_path = os.path.join(os.path.dirname(__file__), "..", "src")
        result = subprocess.run(
            ["python3", "-m", "fhir_parser", bundle_file],
            capture_output=True, text=True, timeout=30,
            env={**os.environ, "PYTHONPATH": src_path},
            cwd=os.path.dirname(__file__) + "/..",
        )
        assert result.returncode == 0, f"stderr: {result.stderr}"
        assert "PATIENT SUMMARY" in result.stdout

    def test_python_m_with_json_flag(self, bundle_file):
        """python -m fhir_parser --json should work."""
        import subprocess, os
        src_path = os.path.join(os.path.dirname(__file__), "..", "src")
        result = subprocess.run(
            ["python3", "-m", "fhir_parser", bundle_file, "--json"],
            capture_output=True, text=True, timeout=30,
            env={**os.environ, "PYTHONPATH": src_path},
            cwd=os.path.dirname(__file__) + "/..",
        )
        assert result.returncode == 0, f"stderr: {result.stderr}"
        data = json.loads(result.stdout)
        assert "patient" in data

    def test_python_m_malformed(self, malformed_bundle_file):
        """python -m fhir_parser --validate-only with malformed bundle."""
        import subprocess, os
        src_path = os.path.join(os.path.dirname(__file__), "..", "src")
        result = subprocess.run(
            ["python3", "-m", "fhir_parser", malformed_bundle_file, "--validate-only"],
            capture_output=True, text=True, timeout=30,
            env={**os.environ, "PYTHONPATH": src_path},
            cwd=os.path.dirname(__file__) + "/..",
        )
        assert result.returncode == 1
        assert "failed" in result.stdout.lower()
