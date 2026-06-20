"""Tests for the DICOM reader — parse round-trip, tag extraction, sequences."""

import os
import struct
import tempfile
from pathlib import Path

import pytest

from medicom.dicom.reader import DICOMFile, DICOMDataset, DataElement
from medicom.dicom.tags import (
    Tag, TAGS, PATIENT_NAME, PATIENT_ID, PATIENT_SEX,
    STUDY_INSTANCE_UID, SERIES_INSTANCE_UID, MODALITY,
    ROWS, COLUMNS, BITS_ALLOCATED, BITS_STORED,
    WINDOW_CENTER, WINDOW_WIDTH, RESCALE_SLOPE, RESCALE_INTERCEPT,
    PIXEL_DATA, TRANSFER_SYNTAX_UID,
    SOP_CLASS_UID, SOP_INSTANCE_UID,
    INSTANCE_NUMBER, PIXEL_SPACING, IMAGE_POSITION_PATIENT,
)
from medicom.generate import generate_dicom, generate_synthetic_series


# ── Fixtures ─────────────────────────────────────────────────────────────────

@pytest.fixture
def synthetic_ct(tmp_path):
    """Generate a minimal CT DICOM file."""
    return generate_dicom(
        output=tmp_path / "test_ct.dcm",
        rows=32, cols=32,
        modality="CT",
        patient_name="Test^Patient",
        patient_id="TEST001",
        rescale_slope=1.0,
        rescale_intercept=-1024.0,
        window_center=40.0,
        window_width=400.0,
        pixel_pattern="circle",
    )


@pytest.fixture
def synthetic_mr(tmp_path):
    """Generate a minimal MR DICOM file."""
    return generate_dicom(
        output=tmp_path / "test_mr.dcm",
        rows=16, cols=16,
        modality="MR",
        patient_name="MR^Patient",
        patient_id="MR001",
        rescale_slope=1.0,
        rescale_intercept=0.0,
        pixel_pattern="gradient",
    )


@pytest.fixture
def synthetic_series(tmp_path):
    """Generate a series of 3 CT slices."""
    return generate_synthetic_series(
        output_dir=tmp_path / "series",
        num_instances=3,
        rows=16, cols=16,
    )


# ── Basic parsing tests ─────────────────────────────────────────────────────

class TestDICOMParsing:
    """Core parsing: preamble, DICM magic, meta, dataset."""

    def test_parse_returns_dicom_file(self, synthetic_ct):
        dcm = DICOMFile.from_path(synthetic_ct)
        assert isinstance(dcm, DICOMFile)
        assert dcm.path == synthetic_ct

    def test_parse_from_bytes(self, synthetic_ct):
        raw = synthetic_ct.read_bytes()
        dcm = DICOMFile.from_bytes(raw)
        assert dcm.dataset.get_str(PATIENT_NAME) == "Test^Patient"

    def test_file_meta_has_transfer_syntax(self, synthetic_ct):
        dcm = DICOMFile.from_path(synthetic_ct)
        ts = dcm.file_meta.get_str(TRANSFER_SYNTAX_UID)
        assert "1.2.840.10008.1.2" in ts

    def test_has_pixel_data(self, synthetic_ct):
        dcm = DICOMFile.from_path(synthetic_ct)
        assert dcm.has_pixel_data()
        assert len(dcm.pixel_array()) > 0

    def test_pixel_data_size_matches(self, synthetic_ct):
        dcm = DICOMFile.from_path(synthetic_ct)
        rows, cols = 32, 32
        bits = 16
        expected = rows * cols * (bits // 8)
        assert len(dcm.pixel_array()) == expected


# ── Tag extraction tests ─────────────────────────────────────────────────────

class TestTagExtraction:
    """Verify correct tag extraction for patient, study, series, image."""

    def test_patient_name(self, synthetic_ct):
        dcm = DICOMFile.from_path(synthetic_ct)
        assert dcm.dataset.get_str(PATIENT_NAME) == "Test^Patient"

    def test_patient_id(self, synthetic_ct):
        dcm = DICOMFile.from_path(synthetic_ct)
        assert dcm.dataset.get_str(PATIENT_ID) == "TEST001"

    def test_modality(self, synthetic_ct):
        dcm = DICOMFile.from_path(synthetic_ct)
        assert dcm.dataset.get_str(MODALITY) == "CT"

    def test_modality_mr(self, synthetic_mr):
        dcm = DICOMFile.from_path(synthetic_mr)
        assert dcm.dataset.get_str(MODALITY) == "MR"

    def test_rows_columns(self, synthetic_ct):
        dcm = DICOMFile.from_path(synthetic_ct)
        assert dcm.dataset.get_int(ROWS) == 32
        assert dcm.dataset.get_int(COLUMNS) == 32

    def test_bits_allocated(self, synthetic_ct):
        dcm = DICOMFile.from_path(synthetic_ct)
        assert dcm.dataset.get_int(BITS_ALLOCATED) == 16

    def test_bits_stored(self, synthetic_ct):
        dcm = DICOMFile.from_path(synthetic_ct)
        assert dcm.dataset.get_int(BITS_STORED) == 12

    def test_window_center_width(self, synthetic_ct):
        dcm = DICOMFile.from_path(synthetic_ct)
        # Window center/width stored as strings — parse them
        wc = float(dcm.dataset.get_str(WINDOW_CENTER))
        ww = float(dcm.dataset.get_str(WINDOW_WIDTH))
        assert wc == pytest.approx(40.0)
        assert ww == pytest.approx(400.0)

    def test_rescale_slope_intercept(self, synthetic_ct):
        dcm = DICOMFile.from_path(synthetic_ct)
        slope = dcm.dataset.get_float(RESCALE_SLOPE, 1.0)
        intercept = dcm.dataset.get_float(RESCALE_INTERCEPT, 0.0)
        assert slope == pytest.approx(1.0)
        assert intercept == pytest.approx(-1024.0)

    def test_study_instance_uid_present(self, synthetic_ct):
        dcm = DICOMFile.from_path(synthetic_ct)
        uid = dcm.dataset.get_str(STUDY_INSTANCE_UID)
        assert len(uid) > 0

    def test_series_instance_uid_present(self, synthetic_ct):
        dcm = DICOMFile.from_path(synthetic_ct)
        uid = dcm.dataset.get_str(SERIES_INSTANCE_UID)
        assert len(uid) > 0

    def test_pixel_spacing(self, synthetic_ct):
        dcm = DICOMFile.from_path(synthetic_ct)
        ps = dcm.dataset.get_str(PIXEL_SPACING)
        assert ps == "0.5\\0.5"

    def test_image_position_patient(self, synthetic_ct):
        dcm = DICOMFile.from_path(synthetic_ct)
        pos = dcm.dataset.get_str(IMAGE_POSITION_PATIENT)
        assert "0.0" in pos

    def test_sop_class_uid_ct(self, synthetic_ct):
        dcm = DICOMFile.from_path(synthetic_ct)
        uid = dcm.dataset.get_str(SOP_CLASS_UID)
        assert len(uid) > 0


# ── Summary tests ────────────────────────────────────────────────────────────

class TestSummary:
    def test_summary_contains_patient_name(self, synthetic_ct):
        dcm = DICOMFile.from_path(synthetic_ct)
        summary = dcm.summary()
        assert "Test^Patient" in summary

    def test_summary_contains_modality(self, synthetic_ct):
        dcm = DICOMFile.from_path(synthetic_ct)
        summary = dcm.summary()
        assert "CT" in summary

    def test_summary_contains_dimensions(self, synthetic_ct):
        dcm = DICOMFile.from_path(synthetic_ct)
        summary = dcm.summary()
        assert "32" in summary


# ── Error handling ───────────────────────────────────────────────────────────

class TestErrorHandling:
    def test_invalid_magic_raises(self, tmp_path):
        bad_file = tmp_path / "bad.dcm"
        bad_file.write_bytes(b"\x00" * 128 + b"NOPE")
        with pytest.raises(ValueError, match="Missing DICM"):
            DICOMFile.from_path(bad_file)

    def test_truncated_file_raises(self, tmp_path):
        short_file = tmp_path / "short.dcm"
        short_file.write_bytes(b"\x00" * 100)
        with pytest.raises(ValueError):
            DICOMFile.from_path(short_file)

    def test_no_pixel_data_raises(self, tmp_path):
        # Generate a file but access pixel_array on a file without pixels
        dcm_path = tmp_path / "no_pixels.dcm"
        # Write minimal DICOM without pixel data
        with open(dcm_path, "wb") as f:
            f.write(b"\x00" * 128)
            f.write(b"DICM")
            # Minimal meta
            import io
            meta = io.BytesIO()
            # Group length placeholder
            f.write(struct.pack("<HH", 0x0002, 0x0000))
            f.write(b"UL")
            meta_bytes = b""  # empty meta data
            f.write(struct.pack("<I", 4))
            f.write(struct.pack("<I", 0))
        with pytest.raises(ValueError, match="No pixel data"):
            DICOMFile.from_path(dcm_path).pixel_array()
