import os
import sys
import tomllib
import unittest
from pathlib import Path
from unittest.mock import patch

from jupyter_ai_acp_client.base_acp_persona import BaseAcpPersona
from jupyter_ai_persona_manager import PersonaRequirementsUnmet

from jupyter_ai_biorouter.persona import (
    BIOROUTER_EXECUTABLE,
    BioRouterAcpPersona,
    resolve_biorouter_executable,
)


class BioRouterPersonaTest(unittest.TestCase):
    def test_defaults(self):
        persona = object.__new__(BioRouterAcpPersona)
        defaults = persona.defaults

        self.assertEqual(defaults.name, "Biorouter")
        self.assertTrue(Path(defaults.avatar_path).is_file())

    def test_explicit_executable(self):
        with patch.dict(
            os.environ, {"BIOROUTER_EXECUTABLE": sys.executable}, clear=False
        ):
            self.assertEqual(
                resolve_biorouter_executable(), str(Path(sys.executable).resolve())
            )

    def test_invalid_explicit_executable(self):
        with patch.dict(
            os.environ,
            {"BIOROUTER_EXECUTABLE": "/missing/biorouter"},
            clear=False,
        ):
            with self.assertRaises(PersonaRequirementsUnmet):
                resolve_biorouter_executable()

    def test_launches_biorouter_acp(self):
        with patch.object(BaseAcpPersona, "__init__", return_value=None) as base_init:
            BioRouterAcpPersona(example="value")

        base_init.assert_called_once_with(
            executable=[BIOROUTER_EXECUTABLE, "acp"],
            example="value",
        )

    def test_manifest_registers_persona(self):
        manifest = Path(__file__).parents[1] / "pyproject.toml"
        project = tomllib.loads(manifest.read_text())["project"]

        self.assertEqual(
            project["entry-points"]["jupyter_ai.personas"]["biorouter-acp"],
            "jupyter_ai_biorouter.persona:BioRouterAcpPersona",
        )


if __name__ == "__main__":
    unittest.main()
