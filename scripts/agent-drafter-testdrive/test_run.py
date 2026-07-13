from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("run.py")
SPEC = importlib.util.spec_from_file_location("agent_drafter_testdrive_run", MODULE_PATH)
assert SPEC and SPEC.loader
RUN = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUN)


class DriverTests(unittest.TestCase):
    def test_locked_corpus_and_ids(self) -> None:
        specs = RUN.parse_specs()
        self.assertEqual([spec["number"] for spec in specs], list(range(1, 101)))
        self.assertEqual(RUN.app_id(specs[0]), "spec-001-variant-tribunal")
        self.assertEqual(RUN.app_id(specs[-1]), "spec-100-resume-atelier")

    def test_ucsf_provider_error_detection(self) -> None:
        rejected = (
            "Authentication error: Authentication failed. Status: 403 Forbidden. "
            'Response: {"error":"The IP Address is invalid: 104.52.5.246"}.'
        )
        self.assertTrue(RUN.is_provider_error(rejected))
        self.assertFalse(RUN.is_provider_error("build_app completed with 0 ERRORs"))

    def test_provider_blocked_rounds_do_not_consume_budget(self) -> None:
        rounds = [
            {"kind": "build", "rc": 0, "duration_s": 100.0},
            {
                "kind": "provider-blocked",
                "rc": 75,
                "duration_s": 5.0,
                "provider_error": True,
            },
            {"kind": "manual-fix", "rc": 0, "duration_s": 90.0},
        ]
        self.assertEqual(
            [round_["kind"] for round_ in RUN.credited_rounds(rounds)],
            ["build", "manual-fix"],
        )

    def test_absent_theme_resolves_to_default_biorouter_pack(self) -> None:
        self.assertEqual(RUN.resolved_theme_pack({}), "biorouter")
        self.assertEqual(
            RUN.resolved_theme_pack({"theme": {"pack": "clinical"}}),
            "clinical",
        )


if __name__ == "__main__":
    unittest.main()
