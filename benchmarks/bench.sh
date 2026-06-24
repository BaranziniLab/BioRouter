#!/usr/bin/env bash
# BioRouter perf benchmark harness (macOS).
# Usage:  benchmarks/bench.sh <label>
# Measures, for the CURRENT built release binaries + workspace:
#   1. release binary sizes (biorouter, biorouterd)
#   2. total resolved crate count (from Cargo.lock, deterministic, disk-free)
#   3. biorouterd idle-boot RSS (launch on a private port, wait for /status, sample)
#   4. biorouterd cold startup time (launch -> first /status 200)
# Appends a row to benchmarks/results/results.tsv and writes a detailed
# benchmarks/results/<label>.md.  Designed for A/B comparison on one machine.
set -uo pipefail

LABEL="${1:?usage: bench.sh <label>}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"
PROFILE="${BENCH_PROFILE:-release}"
BINDIR="$TARGET/$PROFILE"
PORT="${BENCH_PORT:-3997}"
SECRET="${BENCH_SECRET:-test}"
RES="benchmarks/results"
TSV="$RES/results.tsv"
mkdir -p "$RES"

now_ms() { python3 -c 'import time; print(int(time.time()*1000))'; }
hr() { printf '%s\n' "----------------------------------------"; }

# 1. binary sizes -----------------------------------------------------------
sz() { [ -f "$1" ] && stat -f%z "$1" || echo 0; }
BR_BIN="$BINDIR/biorouter"
BD_BIN="$BINDIR/biorouterd"
BR_SZ=$(sz "$BR_BIN"); BD_SZ=$(sz "$BD_BIN")

# 2. crate count ------------------------------------------------------------
# total distinct resolved packages in the dependency graph (Cargo.lock).
CRATES=$(grep -c '^name = ' Cargo.lock 2>/dev/null || echo 0)
# crates actually compiled for the default biorouter build (resolve graph)
BR_DEPS=$(cargo tree -p biorouter --edges normal --prefix none 2>/dev/null | sort -u | grep -c . || echo 0)
BD_DEPS=$(cargo tree -p biorouter-server --edges normal --prefix none 2>/dev/null | sort -u | grep -c . || echo 0)

# 3 + 4. launch biorouterd, measure startup + idle RSS ----------------------
RSS_KB=0; STARTUP_MS=0; STATUS="ok"
if [ -x "$BD_BIN" ]; then
  LOG="$RES/.${LABEL}.biorouterd.log"
  T0=$(now_ms)
  BIOROUTER_SERVER__SECRET_KEY="$SECRET" \
  BIOROUTER_SERVER__PORT="$PORT" \
  BIOROUTER_SERVER__HOST="127.0.0.1" \
    "$BD_BIN" agent >"$LOG" 2>&1 &
  PID=$!
  READY=0
  for _ in $(seq 1 600); do   # up to ~12s (600*20ms)
    if curl -fsS -H "X-Secret-Key: $SECRET" "http://127.0.0.1:$PORT/status" >/dev/null 2>&1; then
      READY=1; break
    fi
    if ! kill -0 "$PID" 2>/dev/null; then STATUS="died"; break; fi
    python3 -c 'import time; time.sleep(0.02)'
  done
  if [ "$READY" = 1 ]; then
    STARTUP_MS=$(( $(now_ms) - T0 ))
    # let it settle, then sample RSS a few times, take the max
    python3 -c 'import time; time.sleep(1.5)'
    MAX=0
    for _ in 1 2 3 4; do
      R=$(ps -o rss= -p "$PID" 2>/dev/null | tr -d ' ')
      [ -n "$R" ] && [ "$R" -gt "$MAX" ] && MAX=$R
      python3 -c 'import time; time.sleep(0.3)'
    done
    RSS_KB=$MAX
  else
    [ "$STATUS" = ok ] && STATUS="timeout"
  fi
  # tear down PID + any children
  pkill -P "$PID" 2>/dev/null
  kill "$PID" 2>/dev/null
  wait "$PID" 2>/dev/null
fi

# report --------------------------------------------------------------------
GITREV=$(git rev-parse --short HEAD 2>/dev/null || echo "?")
if [ ! -f "$TSV" ]; then
  printf 'label\tgit\tbiorouter_bytes\tbiorouterd_bytes\tlockfile_crates\tbiorouter_deps\tbiorouterd_deps\tidle_rss_kb\tstartup_ms\tstatus\n' >"$TSV"
fi
printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
  "$LABEL" "$GITREV" "$BR_SZ" "$BD_SZ" "$CRATES" "$BR_DEPS" "$BD_DEPS" "$RSS_KB" "$STARTUP_MS" "$STATUS" >>"$TSV"

{
  echo "# bench: $LABEL ($GITREV)"
  echo
  echo "| metric | value |"
  echo "|---|---:|"
  echo "| biorouter (release) | $(awk "BEGIN{printf \"%.1f\", $BR_SZ/1048576}") MB |"
  echo "| biorouterd (release) | $(awk "BEGIN{printf \"%.1f\", $BD_SZ/1048576}") MB |"
  echo "| Cargo.lock crates | $CRATES |"
  echo "| biorouter dep crates | $BR_DEPS |"
  echo "| biorouter-server dep crates | $BD_DEPS |"
  echo "| biorouterd idle RSS | $(awk "BEGIN{printf \"%.1f\", $RSS_KB/1024}") MB |"
  echo "| biorouterd startup | $STARTUP_MS ms (status: $STATUS) |"
} | tee "$RES/$LABEL.md"

hr
echo "appended -> $TSV"
