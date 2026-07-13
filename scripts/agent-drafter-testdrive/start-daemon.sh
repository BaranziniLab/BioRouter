#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$ROOT/bin/activate-hermit"
source /tmp/br-testdrive.env

export BIOROUTER_PATH_ROOT="$ROOT/.br-testdrive/runtime"
export XDG_CONFIG_HOME="$ROOT/.br-testdrive/runtime/config"
export BIOROUTER_PROVIDER=versa_azure
export BIOROUTER_MODEL=gpt-5.5-2026-04-24
export BIOROUTER_DISABLE_KEYRING=true
export BIOROUTER_SERVER__SECRET_KEY=test
export BIOROUTER_PORT=8899
export BIOROUTER_ESBUILD_BIN="$ROOT/ui/desktop/node_modules/.bin/esbuild"
export CARGO_TARGET_DIR=/tmp/br-testdrive-target

PID_FILE="$ROOT/.br-testdrive/biorouterd.pid"
LOG_FILE="$ROOT/.br-testdrive/biorouterd.log"

if curl -sf -o /dev/null http://127.0.0.1:8899/status; then
  echo "BioRouter test-drive daemon already ready on :8899"
  exit 0
fi

/tmp/br-testdrive-target/debug/biorouterd agent >"$LOG_FILE" 2>&1 &
pid=$!
echo "$pid" >"$PID_FILE"
for _ in $(seq 1 90); do
  if curl -sf -o /dev/null http://127.0.0.1:8899/status; then
    echo "BioRouter test-drive daemon ready on :8899 (pid $pid)"
    exit 0
  fi
  if ! kill -0 "$pid" 2>/dev/null; then
    echo "BioRouter daemon exited during startup; inspect $LOG_FILE" >&2
    exit 1
  fi
  sleep 1
done

echo "BioRouter daemon did not become ready; inspect $LOG_FILE" >&2
exit 1
