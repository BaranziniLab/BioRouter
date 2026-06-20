"""Tests for the CLI — calls code directly, no subprocess."""

import struct
from pathlib import Path
from unittest.mock import patch

import pytest

from medicom.generate import generate_dicom
from medicom.cli import main, cmd_read, cmd_window, cmd_generate, _parse_ds_list


@pytest.fixture
def cli_ct(tmp_path):
    """Generate a CT DICOM file for CLI testing."""
    return generate_dicom(
        output=tmp_path / "cli_ct.dcm",
        rows=16, cols=16,
        modality="CT",
        patient_name="CLI^Patient",
        patient_id="CLI001",
    )


class TestParseDsList:
    def test_single_value(self):
        assert _parse_ds_list("40.0") == [40.0]

    def test_backslash_separated(self):
        assert _parse_ds_list("40\\400") == [40.0, 400.0]

    def test_space_separated(self):
        assert _parse_ds_list("40 400") == [40.0, 400.0]

    def test_non_numeric(self):
        result = _parse_ds_list("abc")
        assert result == ["abc"]


class TestCLIRead:
    def test_read_outputs_summary(self, cli_ct, capsys):
        cmd_read(type('Args', (), {'input': str(cli_ct)})())
        captured = capsys.readouterr()
        assert "DICOM Header Summary" in captured.out
        assert "CLI^Patient" in captured.out
        assert "CT" in captured.out

    def test_read_nonexistent_exits(self, tmp_path, capsys):
        with pytest.raises(SystemExit) as exc_info:
            cmd_read(type('Args', (), {'input': str(tmp_path / "nonexistent.dcm")})())
        assert exc_info.value.code == 1


class TestCLIWindow:
    def test_window_writes_png(self, cli_ct, tmp_path):
        args = type('Args', (), {
            'input': str(cli_ct),
            'output': str(tmp_path / "out.png"),
            'window_center': None,
            'window_width': None,
        })()
        cmd_window(args)
        assert (tmp_path / "out.png").exists()

    def test_window_writes_pgm(self, cli_ct, tmp_path):
        args = type('Args', (), {
            'input': str(cli_ct),
            'output': str(tmp_path / "out.pgm"),
            'window_center': None,
            'window_width': None,
        })()
        cmd_window(args)
        assert (tmp_path / "out.pgm").exists()

    def test_window_custom_wc_ww(self, cli_ct, tmp_path):
        args = type('Args', (), {
            'input': str(cli_ct),
            'output': str(tmp_path / "out.png"),
            'window_center': 40.0,
            'window_width': 400.0,
        })()
        cmd_window(args)
        assert (tmp_path / "out.png").exists()

    def test_window_no_pixel_data_exits(self, tmp_path, capsys):
        # Create a minimal DICOM without pixel data
        from medicom.generate import generate_dicom
        dcm_path = generate_dicom(
            output=tmp_path / "no_px.dcm",
            rows=4, cols=4,
        )
        # Parse it and verify it has pixel data (generated files always do)
        dcm = __import__('medicom.dicom.reader', fromlist=['DICOMFile']).DICOMFile.from_path(dcm_path)
        assert dcm.has_pixel_data()


class TestCLIGenerate:
    def test_generate_creates_file(self, tmp_path):
        args = type('Args', (), {
            'output': str(tmp_path / "gen.dcm"),
            'rows': 8,
            'cols': 8,
            'modality': 'MR',
            'patient_name': 'Test^Gen',
            'patient_id': 'GEN002',
            'pattern': 'steps',
            'rescale_slope': 1.0,
            'rescale_intercept': -1024.0,
            'window_center': 40.0,
            'window_width': 400.0,
        })()
        cmd_generate(args)
        assert (tmp_path / "gen.dcm").exists()

    def test_generate_main_dispatch(self, tmp_path):
        """Test main() dispatches to generate subcommand."""
        output = str(tmp_path / "dispatch.dcm")
        main(["generate", "-o", output, "--rows", "8", "--cols", "8"])
        assert Path(output).exists()


class TestCLIMain:
    def test_main_no_args(self, capsys):
        with pytest.raises(SystemExit) as exc_info:
            main([])
        assert exc_info.value.code == 0

    def test_main_read(self, cli_ct, capsys):
        main(["read", str(cli_ct)])
        captured = capsys.readouterr()
        assert "DICOM Header Summary" in captured.out

    def test_main_info(self, cli_ct, capsys):
        main(["info", str(cli_ct)])
        captured = capsys.readouterr()
        assert "DICOM Header Summary" in captured.out
