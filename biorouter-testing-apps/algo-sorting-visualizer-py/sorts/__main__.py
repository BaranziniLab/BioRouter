"""
Main entry point for the sorting algorithm visualizer package.

Allows running via: python -m sorts [subcommand] [args]
"""

import sys
from .cli import main

if __name__ == '__main__':
    sys.exit(main())
