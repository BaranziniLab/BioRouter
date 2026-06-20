"""Tests for series loader."""

import pytest

from medicom.generate import generate_synthetic_series
from medicom.series import load_series, load_single_series, DICOMSeries, DICOMInstance
from medicom.dicom.tags import ROWS, COLUMNS, MODALITY


@pytest.fixture
def series_dir(tmp_path):
    """Generate a 3-slice series."""
    paths = generate_synthetic_series(
        output_dir=tmp_path / "series",
        num_instances=3,
        rows=16, cols=16,
        modality="CT",
    )
    return tmp_path / "series", paths


class TestSeriesLoader:
    def test_load_series_finds_files(self, series_dir):
        dir_path, expected = series_dir
        series_map = load_series(dir_path)
        assert len(series_map) == 1

    def test_series_has_correct_uid(self, series_dir):
        dir_path, expected = series_dir
        series_map = load_series(dir_path)
        uid = next(iter(series_map))
        assert len(uid) > 0

    def test_series_sorted_by_position(self, series_dir):
        dir_path, expected = series_dir
        series_map = load_series(dir_path, sort_by="position")
        series = next(iter(series_map.values()))
        positions = [inst.image_position_z for inst in series]
        assert positions == sorted(positions)

    def test_series_sorted_by_instance(self, series_dir):
        dir_path, expected = series_dir
        series_map = load_series(dir_path, sort_by="instance")
        series = next(iter(series_map.values()))
        numbers = [inst.instance_number for inst in series]
        assert numbers == sorted(numbers)

    def test_series_count(self, series_dir):
        dir_path, expected = series_dir
        series_map = load_series(dir_path)
        series = next(iter(series_map.values()))
        assert len(series) == 3

    def test_series_modality(self, series_dir):
        dir_path, expected = series_dir
        series_map = load_series(dir_path)
        series = next(iter(series_map.values()))
        assert series.modality == "CT"

    def test_series_dimensions(self, series_dir):
        dir_path, expected = series_dir
        series_map = load_series(dir_path)
        series = next(iter(series_map.values()))
        assert series.rows == 16
        assert series.cols == 16

    def test_single_series_load(self, series_dir):
        dir_path, expected = series_dir
        series = load_single_series(dir_path)
        assert len(series) == 3

    def test_load_single_file(self, series_dir):
        dir_path, expected = series_dir
        # Load just one file
        series = load_single_series(expected[0])
        assert len(series) == 1

    def test_load_nonexistent_path(self, tmp_path):
        with pytest.raises(FileNotFoundError):
            load_series(tmp_path / "nonexistent")

    def test_instance_metadata(self, series_dir):
        dir_path, expected = series_dir
        series_map = load_series(dir_path)
        series = next(iter(series_map.values()))
        inst = series[0]
        assert isinstance(inst, DICOMInstance)
        assert inst.rows == 16
        assert inst.cols == 16

    def test_iteration(self, series_dir):
        dir_path, expected = series_dir
        series_map = load_series(dir_path)
        series = next(iter(series_map.values()))
        count = 0
        for inst in series:
            count += 1
        assert count == 3

    def test_indexing(self, series_dir):
        dir_path, expected = series_dir
        series_map = load_series(dir_path)
        series = next(iter(series_map.values()))
        assert series[0].instance_number == 1
        assert series[1].instance_number == 2
        assert series[2].instance_number == 3
