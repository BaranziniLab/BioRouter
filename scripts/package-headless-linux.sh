#!/usr/bin/env bash
# One-command headless Linux release artifact build:
# build Rust/browser deliverables, verify privacy boundaries, and create a tarball.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

"$ROOT/scripts/build-headless-linux.sh"
"$ROOT/scripts/verify-headless-artifact.sh" --tar
