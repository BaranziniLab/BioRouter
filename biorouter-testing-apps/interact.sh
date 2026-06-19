#!/usr/bin/env bash
# interact.sh <app-folder> <turn-label> <instruction-text>
# Phase 2+ of an INTERACTIVE build: the Claude harness drives a follow-up turn
# against the app's existing BioRouter session (--resume), mimicking a real user
# iterating on their project. Each turn is committed separately so the
# refinement history is visible in git.
set -uo pipefail
export PATH="$HOME/.local/bin:$PATH"

ROOT="/Users/wanjun/Desktop/biorouter-testing-apps"
APP="$1"; TURN="$2"; INSTRUCTION="$3"
DIR="$ROOT/$APP"
TIMEOUT_SECS="${TIMEOUT_SECS:-900}"
cd "$DIR" || { echo "RESULT phase=$TURN app=$APP rc=99 (no dir)"; exit 2; }

LOG="$DIR/interact_${TURN}.log"
CTX="You are iterating on the EXISTING project in this directory ('$APP'). Inspect the current files first, then: $INSTRUCTION"
# Try to resume the session; if none exists, seed a fresh named session so the
# refinement still runs (and is resumable next time).
perl -e 'alarm shift; exec @ARGV' "$TIMEOUT_SECS" \
  biorouter run --name "$APP" --resume -t "$INSTRUCTION" > "$LOG" 2>&1
RC=$?
if grep -q "No session found with name" "$LOG"; then
  echo "[interact] no resumable session; seeding a new named session" >> "$LOG"
  perl -e 'alarm shift; exec @ARGV' "$TIMEOUT_SECS" \
    biorouter run --name "$APP" -t "$CTX" >> "$LOG" 2>&1
  RC=$?
fi

if [ -n "$(git status --porcelain)" ]; then
  git add -A; git commit -q -m "iterate($TURN): $(echo "$INSTRUCTION" | head -c 60)" 2>/dev/null
fi
COMMITS=$(git rev-list --count HEAD 2>/dev/null || echo 0)
FILES=$(git ls-files | wc -l | tr -d ' ')
LOC=$(git ls-files | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}')
echo "RESULT phase=$TURN app=$APP rc=$RC commits=$COMMITS files=$FILES loc=${LOC:-0}"
