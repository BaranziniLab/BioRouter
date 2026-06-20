#!/usr/bin/env bash
# build_app.sh <app-folder> <language> <spec-file>
# Phase 1 of an INTERACTIVE build: drives the BioRouter CLI (Xiaomi MiMo) to do
# the initial build of one app in its own git repo, using a NAMED, resumable
# session so the Claude harness can drive follow-up refinement turns afterward.
set -uo pipefail
export PATH="$HOME/.local/bin:$PATH"

ROOT="${BIOROUTER_TESTING_ROOT:-/Users/wanjun/Desktop/BioRouter/biorouter-testing-apps}"
APP="$1"; LANG_="$2"; SPEC_FILE="$3"
# Resolve spec to an ABSOLUTE path BEFORE any cd (harness bug fix #1).
SPEC_FILE="$(cd "$(dirname "$SPEC_FILE")" && pwd)/$(basename "$SPEC_FILE")"
DIR="$ROOT/$APP"
TIMEOUT_SECS="${TIMEOUT_SECS:-1500}"

mkdir -p "$DIR"; cd "$DIR" || exit 2
if [ ! -d .git ]; then
  git init -q
  git config user.name "BioRouter Build Bot"
  git config user.email "build-bot@biorouter.test"
fi
# Keep harness logs + build artifacts out of commits (local exclude, not tracked).
printf '%s\n' build.log 'interact_*.log' 'target/' '__pycache__/' '*.pyc' 'build/' '.venv/' > .git/info/exclude

SPEC="$(cat "$SPEC_FILE")"
PROMPT="You are building a substantial, real software project named '$APP' in the current directory (an initialized git repo). Language: $LANG_.

$SPEC

Hard requirements:
- MULTI-FILE project (a dozen+ files, hundreds-to-thousands of LOC); not a single script.
- Include a README.md, source split across modules, a test suite, and the standard manifest (Cargo.toml / pyproject.toml or requirements.txt / CMakeLists.txt / DESCRIPTION).
- Build/compile and run the tests with the shell tool; fix errors until it builds and tests pass (or document a missing toolchain).
- Use git: make at least 3 logical commits with clear messages as you finish components.
- Write tests INCREMENTALLY: as you finish each module, immediately add its tests, run them, and commit — do NOT defer the entire test suite to the end.
- Use the todo tool to plan and track the build.
Work autonomously to completion. Do not ask questions."

perl -e 'alarm shift; exec @ARGV' "$TIMEOUT_SECS" \
  biorouter run --name "$APP" -t "$PROMPT" > "$DIR/build.log" 2>&1
RC=$?

cd "$DIR"
if [ -n "$(git status --porcelain)" ]; then
  git add -A; git commit -q -m "chore: capture initial build artifacts for $APP" 2>/dev/null
fi
COMMITS=$(git rev-list --count HEAD 2>/dev/null || echo 0)
FILES=$(git ls-files | wc -l | tr -d ' ')
LOC=$(git ls-files | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}')
echo "RESULT phase=build app=$APP rc=$RC commits=$COMMITS files=$FILES loc=${LOC:-0}"
