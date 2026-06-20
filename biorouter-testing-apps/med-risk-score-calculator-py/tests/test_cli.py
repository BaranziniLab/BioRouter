"""Tests for the CLI interface."""
import json
import pytest
from med_risk_scores.cli import main, _format_result_text
from med_risk_scores.engine import compute
from med_risk_scores.registry import ScoreResult, RiskCategory


class TestCLIListCommand:
    def test_list_scores(self, capsys):
        ret = main(["list"])
        assert ret == 0
        captured = capsys.readouterr()
        assert "cha2ds2_vasc" in captured.out
        assert "has_bled" in captured.out
        assert "CHA₂DS₂-VASc" in captured.out

    def test_list_output_has_header(self, capsys):
        ret = main(["list"])
        captured = capsys.readouterr()
        assert "Display Name" in captured.out


class TestCLIInfoCommand:
    def test_info_cha2ds2(self, capsys):
        ret = main(["info", "cha2ds2_vasc"])
        assert ret == 0
        captured = capsys.readouterr()
        assert "CHA₂DS₂-VASc" in captured.out
        assert "age" in captured.out
        assert "chf" in captured.out

    def test_info_shows_categories(self, capsys):
        ret = main(["info", "curb65"])
        captured = capsys.readouterr()
        assert "Risk categories" in captured.out
        assert "Low risk" in captured.out

    def test_info_shows_references(self, capsys):
        ret = main(["info", "wells_pe"])
        captured = capsys.readouterr()
        assert "References" in captured.out

    def test_info_unknown_score(self, capsys):
        ret = main(["info", "nonexistent_xyz"])
        # Should raise KeyError
        assert ret != 0 or "nonexistent" in capsys.readouterr().out.lower()


class TestCLIComputeCommand:
    def test_compute_cha2ds2(self, capsys):
        ret = main(["compute", "cha2ds2_vasc",
                     "--chf", "0", "--hypertension", "1", "--age", "72",
                     "--diabetes", "1", "--stroke-tia", "0",
                     "--vascular-disease", "0", "--sex-female", "1"])
        assert ret == 0
        captured = capsys.readouterr()
        assert "Score:" in captured.out
        assert "cha2ds2_vasc" in captured.out

    def test_compute_qsofa(self, capsys):
        ret = main(["compute", "qsofa",
                     "--respiratory-rate", "25",
                     "--altered-mentation", "true",
                     "--systolic-bp", "90"])
        assert ret == 0
        captured = capsys.readouterr()
        assert "3" in captured.out
        assert "High risk" in captured.out

    def test_compute_json_output(self, capsys, monkeypatch):
        inputs = {"respiratory_rate": 25, "altered_mentation": True, "systolic_bp": 90}
        monkeypatch.setattr("sys.stdin", __import__("io").StringIO(json.dumps(inputs)))
        ret = main(["compute", "qsofa", "--json"])
        assert ret == 0
        captured = capsys.readouterr()
        data = json.loads(captured.out)
        assert data["score_name"] == "qsofa"
        assert data["total_score"] == 3.0

    def test_compute_pretty_json(self, capsys, monkeypatch):
        inputs = {"confusion": True, "bun": 25, "respiratory_rate": 35,
                  "systolic_bp": 80, "diastolic_bp": 50, "age": 80}
        monkeypatch.setattr("sys.stdin", __import__("io").StringIO(json.dumps(inputs)))
        ret = main(["compute", "curb65", "--json", "--pretty"])
        assert ret == 0
        captured = capsys.readouterr()
        data = json.loads(captured.out)
        assert data["total_score"] == 5
        assert data["risk_label"] == "Very high risk (4-5)"

    def test_compute_with_all_flag(self, capsys):
        ret = main(["compute", "cha2ds2_vasc", "--all",
                     "--chf", "1", "--hypertension", "1", "--age", "80",
                     "--diabetes", "1", "--stroke-tia", "1",
                     "--vascular-disease", "1", "--sex-female", "1"])
        assert ret == 0
        captured = capsys.readouterr()
        assert "Contributions:" in captured.out

    def test_compute_validation_error(self, capsys):
        """Missing required inputs should fail gracefully."""
        ret = main(["compute", "cha2ds2_vasc"])
        assert ret == 1
        captured = capsys.readouterr()
        assert "error" in captured.err.lower() or "missing" in captured.err.lower()

    def test_compute_json_stdin(self, capsys, monkeypatch):
        """Compute from JSON on stdin."""
        inputs = {"respiratory_rate": 25, "altered_mentation": True, "systolic_bp": 90}
        monkeypatch.setattr("sys.stdin", __import__("io").StringIO(json.dumps(inputs)))
        ret = main(["compute", "qsofa", "--json"])
        assert ret == 0
        captured = capsys.readouterr()
        data = json.loads(captured.out)
        assert data["total_score"] == 3.0

    def test_compute_invalid_json_stdin(self, capsys, monkeypatch):
        monkeypatch.setattr("sys.stdin", __import__("io").StringIO("not json"))
        ret = main(["compute", "qsofa", "--json"])
        assert ret == 1

    def test_default_command_shows_help(self, capsys):
        ret = main([])
        assert ret == 0


class TestFormatResultText:
    def test_format_result(self):
        cat = RiskCategory(min_score=0, max_score=3, label="Low", interpretation="Low risk")
        r = ScoreResult(
            score_name="test_score", total_score=2, category=cat,
            contributions={"factor_a": 1.0, "factor_b": 1.0},
            raw_inputs={}, messages=[],
        )
        text = _format_result_text(r, show_all=True)
        assert "test_score" in text
        assert "2" in text
        assert "Low" in text
        assert "factor_a" in text

    def test_format_with_messages(self):
        cat = RiskCategory(min_score=0, max_score=9, label="High", interpretation="High")
        r = ScoreResult(
            score_name="test", total_score=9, category=cat,
            contributions={"x": 5.0, "y": 4.0},
            raw_inputs={}, messages=["Note: total != sum"],
        )
        text = _format_result_text(r, show_all=True)
        assert "Note:" in text
