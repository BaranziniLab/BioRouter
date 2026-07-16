#!/usr/bin/env bash
#
# verify-and-checkpoint.sh — an opt-in Biorouter **Stop hook**.
#
# When the agent is about to finish a turn inside a git repository, this hook:
#   1. (cheap, always) checks the work is committed — a result that builds "in my
#      session" but leaves uncommitted changes is not reproducible from a clean
#      checkout.
#   2. (opt-in, BIOROUTER_VERIFY_BUILD=1) builds + tests the project for its
#      detected toolchain (Cargo / CMake / pytest / npm) and refuses to finish on
#      a broken build or red tests.
#
# If either check fails it prints a `{"decision":"block","reason":...}` document
# on stdout, which Biorouter feeds back to the agent so it fixes/commits before
# stopping. The runtime caps consecutive Stop-hook blocks, so this cannot loop
# forever. The hook is FAILURE-OPEN: outside a git repo, or on any internal
# error, it allows the stop.
#
# Motivation: in QA the agent frequently declared "done" on a non-building C++
# project (never ran cmake), shipped red Rust tests, or left everything
# uncommitted. This hook turns "hope it's reproducible" into "checked".
#
# Wire it up (see docs/hooks/verify-and-checkpoint.md):
#   "hooks": { "Stop": [ { "hooks": [ { "type": "command",
#     "command": "/abs/path/to/scripts/hooks/verify-and-checkpoint.sh" } ] } ] }
#
# Env:
#   BIOROUTER_VERIFY_BUILD=1   enable the build/test check (off by default — it
#                              can be slow; the commit check is always on)
#   BIOROUTER_SKIP_VERIFY_HOOK=1  disable the hook entirely
set -uo pipefail

allow() { exit 0; }

# JSON-escape a string (handles \, ", newlines, tabs) without external deps.
json_escape() {
  local s=$1
  s=${s//\\/\\\\}
  s=${s//\"/\\\"}
  s=${s//$'\n'/\\n}
  s=${s//$'\t'/\\t}
  printf '%s' "$s"
}

block() { printf '{"decision":"block","reason":"%s"}\n' "$(json_escape "$1")"; exit 0; }

[ "${BIOROUTER_SKIP_VERIFY_HOOK:-}" = "1" ] && allow

# Only act inside a git work tree.
git rev-parse --is-inside-work-tree >/dev/null 2>&1 || allow
ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || allow
cd "$ROOT" 2>/dev/null || allow

LOG="$(mktemp 2>/dev/null || echo /tmp/_vc.$$)"
trap 'rm -f "$LOG"' EXIT

# ---- (2) opt-in build/test verification --------------------------------------
if [ "${BIOROUTER_VERIFY_BUILD:-}" = "1" ]; then
  fail=""
  if [ -f Cargo.toml ]; then
    cargo test --quiet >"$LOG" 2>&1 || fail="cargo test"
  elif [ -f CMakeLists.txt ]; then
    if rm -rf build && cmake -S . -B build >"$LOG" 2>&1 && cmake --build build >>"$LOG" 2>&1; then
      # Prefer ctest; but C++ projects frequently forget to register tests with
      # add_test(), so if ctest finds none, fall back to running any built
      # executable whose name contains "test" (the common convention).
      ran_ctest=0
      if command -v ctest >/dev/null 2>&1; then
        ctest_out="$(cd build && ctest --output-on-failure 2>&1)"
        echo "$ctest_out" >>"$LOG"
        if printf '%s' "$ctest_out" | grep -qiE "No tests were found"; then
          ran_ctest=0
        else
          ran_ctest=1
          printf '%s' "$ctest_out" | grep -qiE "tests failed|[1-9][0-9]* failed" && fail="ctest"
        fi
      fi
      if [ -z "$fail" ] && [ "$ran_ctest" = "0" ]; then
        while IFS= read -r tb; do
          [ -x "$tb" ] || continue
          if ! "$tb" >>"$LOG" 2>&1; then fail="test binary $(basename "$tb")"; break; fi
        done < <(find build -maxdepth 2 -type f -perm -u+x -name '*test*' 2>/dev/null)
      fi
    else
      fail="cmake build"
    fi
  elif [ -f pyproject.toml ] || [ -f setup.py ] || compgen -G "tests/*.py" >/dev/null 2>&1; then
    python3 -m pytest -q >"$LOG" 2>&1 || fail="pytest"
  elif [ -f package.json ]; then
    npm test --silent >"$LOG" 2>&1 || fail="npm test"
  fi
  if [ -n "$fail" ]; then
    block "Project build/tests are not green ($fail failed). Do not finish yet: diagnose and fix the failures, then re-run the build/tests until they pass. Last output:
$(tail -25 "$LOG")"
  fi
fi

# ---- (1) always: reproducibility / commit check ------------------------------
DIRTY="$(git status --porcelain 2>/dev/null)"
if [ -n "$DIRTY" ]; then
  COUNT="$(printf '%s\n' "$DIRTY" | grep -c .)"
  block "There are $COUNT uncommitted change(s); the result is not reproducible from a clean checkout. Add a .gitignore for build artifacts if needed, then stage and commit your work in logical commits with clear messages before finishing.
$(printf '%s\n' "$DIRTY" | head -15)"
fi

allow
