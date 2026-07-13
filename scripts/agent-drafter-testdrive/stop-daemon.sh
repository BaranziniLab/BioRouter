#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PID_FILE="$ROOT/.br-testdrive/biorouterd.pid"
if [[ -f "$PID_FILE" ]]; then
  pid="$(cat "$PID_FILE")"
  kill "$pid" 2>/dev/null || true
  rm -f "$PID_FILE"
fi

