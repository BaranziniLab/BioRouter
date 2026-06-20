"""Tests for the synthetic DICOM generator."""

import struct
from pathlib import Path

import pytest

from medicom.generate import (
    generate_dicom,
    generate_synthetic_series,
)
from medicom.dicom.reader import DICOMFile
from medicom.dicom.tags import (
    PATIENT_NAME, PATIENT_ID, MODALITY, ROWS, COLUMNS,
    BITS_ALLOCATED, BITS_STORED,
)


@pytest.fixture
def gen_ct(tmp_path):
    """Generate a CT DICOM file."""
    return generate_dicom(
        output=tmp_path / "gen_ct.dcm",
        rows=16, cols=16,
        modality="CT",
        patient_name="Gen^Patient",
        patient_id="GEN001",
    )


@pytest.fixture
def gen_mr(tmp_path):
    """Generate an MR DICOM file."""
    return generate_dicom(
        output=tmp_path / "gen_mr.dcm",
        rows=8, cols=8,
        modality="MR",
        patient_name="MR^Gen",
        patient_id="MRG001",
        pixel_pattern="gradient",
    )


class TestGenerateDicom:
    def test_file_created(self, gen_ct):
        assert gen_ct.exists()
        assert gen_ct.stat().st_size > 0

    def test_starts_with_preamble(self, gen_ct):
        first_132 = gen_ct.read_bytes()[:132]
        assert first_132[:128] == b"\x00" * 128
        assert first_132[128:132] == b"DICM"

    def test_parseable(self, gen_ct):
        dcm = DICOMFile.from_path(gen_ct)
        assert dcm.dataset.get_str(PATIENT_NAME) == "Gen^Patient"
        assert dcm.dataset.get_str(PATIENT_ID) == "GEN001"
        assert dcm.dataset.get_str(MODALITY) == "CT"

    def test_rows_cols(self, gen_ct):
        dcm = DICOMFile.from_path(gen_ct)
        assert dcm.dataset.get_int(ROWS) == 16
        assert dcm.dataset.get_int(COLUMNS) == 16

    def test_pixel_data_present(self, gen_ct):
        dcm = DICOMFile.from_path(gen_ct)
        assert dcm.has_pixel_data()
        assert len(dcm.pixel_array()) == 16 * 16 * 2

    def test_mr_modality(self, gen_mr):
        dcm = DICOMFile.from_path(gen_mr)
        assert dcm.dataset.get_str(MODALITY) == "MR"

    def test_pixel_pattern_circle(self, tmp_path):
        path = generate_dicom(
            output=tmp_path / "circle.dcm",
            rows=32, cols=32, pixel_pattern="circle",
        )
        dcm = DICOMFile.from_path(path)
        pixels = dcm.pixel_array()
        values = list(struct.unpack(f"<{len(pixels)//2}H", pixels))
        # Corners should be lower (air) and center should be higher (tissue)
        center = 16 * 32 + 16  # center pixel index
        corner = 0  # top-left pixel index
        assert values[center] > values[corner]

    def test_pixel_pattern_steps(self, tmp_path):
        path = generate_dicom(
            output=tmp_path / "steps.dcm",
            rows=4, cols=4, pixel_pattern="steps",
        )
        dcm = DICOMFile.from_path(path)
        pixels = dcm.pixel_array()
        values = list(struct.unpack(f"<{len(pixels)//2}H", pixels))
        # Steps pattern: first column should be 0, last should be max
        assert values[0] <= values[3]

    def test_pixel_pattern_checker(self, tmp_path):
        path = generate_dicom(
            output=tmp_path / "checker.dcm",
            rows=16, cols=16, pixel_pattern="checker",
        )
        dcm = DICOMFile.from_path(path)
        assert dcm.has_pixel_data()

    def test_pixel_pattern_uniform(self, tmp_path):
        path = generate_dicom(
            output=tmp_path / "uniform.dcm",
            rows=4, cols=4, pixel_pattern="uniform",
        )
        dcm = DICOMFile.from_path(path)
        pixels = dcm.pixel_array()
        values = list(struct.unpack(f"<{len(pixels)//2}H", pixels))
        assert all(v == values[0] for v in values)

    def test_custom_uids(self, tmp_path):
        path = generate_dicom(
            output=tmp_path / "custom.dcm",
            rows=4, cols=4,
            study_uid="1.2.3.4.5",
            series_uid="1.2.3.4.6",
            instance_uid="1.2.3.4.7",
        )
        dcm = DICOMFile.from_path(path)
        assert "1.2.3.4.5" in dcm.dataset.get_str(
            __import__('medicom.dicom.tags', fromlist=['STUDY_INSTANCE_UID']).STUDY_INSTANCE_UID
        )

    def test_roundtrip_parse_write(self, tmp_path):
        """Generate → parse → verify pixel data integrity."""
        path = generate_dicom(
            output=tmp_path / "rt.dcm",
            rows=8, cols=8, pixel_pattern="uniform",
        )
        dcm = DICOMFile.from_path(path)
        pixels = dcm.pixel_array()
        values = list(struct.unpack(f"<{len(pixels)//2}H", pixels))
        expected_val = (2**12 - 1) // 2  # uniform = max_val // 2
        assert all(v == expected_val for v in values)


class TestGenerateSyntheticSeries:
    def test_creates_correct_number(self, tmp_path):
        paths = generate_synthetic_series(
            output_dir=tmp_path / "series",
            num_instances=5,
            rows=8, cols=8,
        )
        assert len(paths) == 5
        assert all(p.exists() for p in paths)

    def test_files_are_parseable(self, tmp_path):
        paths = generate_synthetic_series(
            output_dir=tmp_path / "series",
            num_instances=3,
            rows=8, cols=8,
        )
        for path in paths:
            dcm = DICOMFile.from_path(path)
            assert dcm.has_pixel_data()

    def test_same_study_uid(self, tmp_path):
        from medicom.dicom.tags import STUDY_INSTANCE_UID
        paths = generate_synthetic_series(
            output_dir=tmp_path / "series",
            num_instances=3,
            rows=8, cols=8,
        )
        study_uids = set()
        for path in paths:
            dcm = DICOMFile.from_path(path)
            study_uids.add(dcm.dataset.get_str(STUDY_INSTANCE_UID))
        assert len(study_uids) == 1

    def test_same_series_uid(self, tmp_path):
        from medicom.dicom.tags import SERIES_INSTANCE_UID
        paths = generate_synthetic_series(
            output_dir=tmp_path / "series",
            num_instances=3,
            rows=8, cols=8,
        )
        series_uids = set()
        for path in paths:
            dcm = DICOMFile.from_path(path)
            series_uids.add(dcm.dataset.get_str(SERIES_INSTANCE_UID))
        assert len(series_uids) == 1

    def test_incrementing_instance_numbers(self, tmp_path):
        from medicom.dicom.tags import INSTANCE_NUMBER
        paths = generate_synthetic_series(
            output_dir=tmp_path / "series",
            num_instances=3,
            rows=8, cols=8,
        )
        numbers = []
        for path in paths:
            dcm = DICOMFile.from_path(path)
            numbers.append(dcm.dataset.get_int(INSTANCE_NUMBER))
        assert numbers == [1, 2, 3]
