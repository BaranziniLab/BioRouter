#!/usr/bin/env bash
# Shared prerequisite-failure reporting for Biorouter's setup and build scripts.
#
# A script that dies with "missing dependency: jq" has told the user what is
# wrong and nothing about how to get past it. Biorouter ships an agent with shell
# access that is good at exactly this, so every prerequisite failure ends with the
# one command that hands the problem to it:
#
#     biorouter doctor --fix <name>
#
# which opens a session already briefed on the dependency, this machine's
# environment, and what to run to prove the fix worked. Same briefing the desktop
# app's "Debug with Biorouter" button produces (see crates/biorouter/src/system.rs).
#
# Source it, then use `br_require_command` or `br_dependency_die`:
#
#     . "$(dirname "${BASH_SOURCE[0]}")/lib/dependency-hint.sh"
#     br_require_command jq        "run-benchmarks"
#     br_require_command docker    "build-headless-linux" "Docker Desktop must also be running"
#
# Falls back to a plain message when no `biorouter` is reachable — a machine
# without the CLI is exactly the machine that cannot run the hint.

# Name printed in the error prefix. Callers may override before sourcing.
: "${BR_HINT_LABEL:=biorouter}"

br_hint_supported() {
  command -v biorouter >/dev/null 2>&1
}

# Print the "Biorouter can debug this" line for a dependency, if it can help.
# $1 = dependency name as `biorouter doctor` knows it (git, uv, python, node, …).
br_dependency_hint() {
  local dep="${1:-}"
  if br_hint_supported; then
    printf '\n  Biorouter can diagnose this for you:\n    biorouter doctor --fix %s\n' "$dep" >&2
  else
    printf '\n  (Install the Biorouter CLI to get `biorouter doctor --fix %s`, which\n   diagnoses a failing prerequisite for you.)\n' "$dep" >&2
  fi
}

# Fail with a dependency-shaped message plus the hint.
# $1 = dependency name, $2 = human explanation, $3... = extra detail lines.
br_dependency_die() {
  local dep="${1:?br_dependency_die needs a dependency name}"
  local message="${2:-missing dependency: $dep}"
  shift 2 || true
  printf '\033[1;31m[%s] %s\033[0m\n' "$BR_HINT_LABEL" "$message" >&2
  local line
  for line in "$@"; do
    printf '  %s\n' "$line" >&2
  done
  br_dependency_hint "$dep"
  exit 1
}

# Assert a command exists, dying with the hint if it does not.
# $1 = command, $2... = extra detail lines.
br_require_command() {
  local cmd="${1:?br_require_command needs a command}"
  shift || true
  command -v "$cmd" >/dev/null 2>&1 || br_dependency_die "$cmd" "missing dependency: $cmd" "$@"
}
