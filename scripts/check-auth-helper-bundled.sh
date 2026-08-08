#!/usr/bin/env bash
# Assert the macOS auth helper is inside a packaged app, WHERE THE DAEMON LOOKS.
#
# ⚠ This exists because the two obvious tests both passed while the shipped path
# was wrong. The unit tests never touch the filesystem layout, and the
# developer-machine test set BIOROUTER_AUTHPROMPT_APP, which bypasses the lookup
# entirely. `extraResource: ['src/bin']` copies the DIRECTORY, so the bundle
# lands in Contents/Resources/bin/ — beside biorouterd, not above it.
#
# Getting that wrong costs nothing visible: the daemon silently falls back to an
# in-process call that cannot work under the desktop app, and macOS users get the
# 60-second refusal the helper was built to remove. Only opening the built .app
# catches it, so that is what this does.
set -euo pipefail

APP="${1:?usage: check-auth-helper-bundled.sh <path to Biorouter.app>}"
DAEMON="$APP/Contents/Resources/bin/biorouterd"

if [ ! -x "$DAEMON" ]; then
  echo "FAIL: no biorouterd at $DAEMON"
  exit 1
fi

# Exactly the two places helper_app() looks, in the same order.
BESIDE="$(dirname "$DAEMON")/Biorouter Authentication.app"
ABOVE="$(dirname "$(dirname "$DAEMON")")/Biorouter Authentication.app"

for CAND in "$BESIDE" "$ABOVE"; do
  if [ -d "$CAND" ]; then
    BIN="$CAND/Contents/MacOS/biorouter-authprompt"
    if [ ! -x "$BIN" ]; then
      echo "FAIL: bundle present but no executable at $BIN"
      exit 1
    fi
    # An arm64 helper in an Intel app is the same class of bug as an arm64
    # backend in an Intel app: it packages cleanly and dies on the user's Mac.
    WANT=$(file -b "$DAEMON" | grep -oE 'arm64|x86_64' | head -1)
    GOT=$(file -b "$BIN" | grep -oE 'arm64|x86_64' | head -1)
    if [ "$WANT" != "$GOT" ]; then
      echo "FAIL: daemon is $WANT but the helper is $GOT"
      exit 1
    fi
    echo "OK: auth helper found where the daemon looks ($GOT)"
    exit 0
  fi
done

echo "FAIL: no 'Biorouter Authentication.app' beside or above $DAEMON"
echo "      macOS users would get the 60s refusal with nothing reporting why."
exit 1
