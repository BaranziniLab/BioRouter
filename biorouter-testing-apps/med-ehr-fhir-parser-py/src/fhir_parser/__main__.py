"""Allow `python -m fhir_parser` to run the CLI."""

from .cli import main
import sys

sys.exit(main())
