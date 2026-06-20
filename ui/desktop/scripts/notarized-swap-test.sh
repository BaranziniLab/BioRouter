#!/usr/bin/env bash
# Live notarized auto-update SWAP test.
#
# Proves the one step that headless tests cannot: macOS Squirrel.Mac validates a
# downloaded *notarized* build's code signature and performs the in-place swap +
# relaunch when the app calls quitAndInstall.
#
# Setup (built beforehand by the caller):
#   $NEW_ZIP  — signed+notarized BioRouter.app zip at the NEW version (payload)
#   $OLD_APP  — signed+notarized BioRouter.app at the OLD version (under test)
#
# Flow: serve NEW + latest-mac.yml over local HTTP → run a copy of OLD pointed at
# that feed with BIOROUTER_UPDATE_AUTO_INSTALL=1 → poll OLD's on-disk version
# until it flips to NEW (swap succeeded) or times out.
#
# Config is sandboxed via XDG_CONFIG_HOME so the user's real ~/.config/biorouter
# is never touched.
set -uo pipefail

DESK="$(cd "$(dirname "$0")/.." && pwd)"
NEW_VER="${NEW_VER:-1.86.0}"
OLD_VER="${OLD_VER:-1.85.5}"
NEW_ZIP="${NEW_ZIP:-$DESK/out/make/zip/darwin/arm64/BioRouter-darwin-arm64-$NEW_VER.zip}"
OLD_APP="${OLD_APP:-$DESK/test-builds/old/BioRouter.app}"
PORT="${PORT:-8799}"

WORK="$(mktemp -d /tmp/br-swap.XXXXXX)"
SERVE="$WORK/serve"; RUNDIR="$WORK/run"; CFG="$WORK/config"
mkdir -p "$SERVE" "$RUNDIR" "$CFG/biorouter"

# Seed the sandboxed config with the real (small) config so the backend starts
# and the app reaches the updater — but NOT the 624MB extensions, and never the
# user's real ~/.config (left untouched).
REAL_CFG="$HOME/.config/biorouter"
[ -f "$REAL_CFG/config.yaml" ] && cp "$REAL_CFG/config.yaml" "$CFG/biorouter/" 2>/dev/null
for d in skills workflows databricks; do
  [ -d "$REAL_CFG/$d" ] && cp -R "$REAL_CFG/$d" "$CFG/biorouter/" 2>/dev/null
done

cleanup() {
  [ -n "${SRV_PID:-}" ] && kill "$SRV_PID" 2>/dev/null
  [ -n "${APP_PID:-}" ] && kill "$APP_PID" 2>/dev/null
  pkill -f "$RUNDIR/BioRouter.app" 2>/dev/null
}
trap cleanup EXIT

echo "==> building update feed (latest-mac.yml + zip) from $NEW_ZIP"
[ -f "$NEW_ZIP" ] || { echo "FAIL: NEW_ZIP missing: $NEW_ZIP"; exit 1; }
cp "$NEW_ZIP" "$SERVE/"
( cd "$DESK" && node scripts/generate-update-manifests.js \
    --version "$NEW_VER" --arm64-zip "$SERVE/$(basename "$NEW_ZIP")" --out "$SERVE" >/dev/null )
[ -f "$SERVE/latest-mac.yml" ] || { echo "FAIL: manifest not generated"; exit 1; }
echo "    manifest:"; sed 's/^/      /' "$SERVE/latest-mac.yml"

echo "==> staging OLD app ($OLD_VER) into writable run dir"
[ -d "$OLD_APP" ] || { echo "FAIL: OLD_APP missing: $OLD_APP"; exit 1; }
cp -R "$OLD_APP" "$RUNDIR/"
APP="$RUNDIR/BioRouter.app"
PLIST="$APP/Contents/Info.plist"
BEFORE="$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$PLIST")"
echo "    staged version before update: $BEFORE"
[ "$BEFORE" = "$OLD_VER" ] || echo "    (note: expected $OLD_VER)"

echo "==> starting local update server on :$PORT"
( cd "$SERVE" && python3 -m http.server "$PORT" >/dev/null 2>&1 ) &
SRV_PID=$!
sleep 1
curl -fsS "http://127.0.0.1:$PORT/latest-mac.yml" >/dev/null || { echo "FAIL: server not serving"; exit 1; }

echo "==> launching OLD app pointed at local feed (auto-install on download)"
BIN="$APP/Contents/MacOS/BioRouter"
USERDATA="$WORK/userdata"; mkdir -p "$USERDATA"
APPLOG="$USERDATA/logs/main.log"   # electron-log writes here (packaged builds)
# --user-data-dir isolates this test instance's state (logs, updater cache,
# window state) from the user's running BioRouter — no shared-state collisions.
XDG_CONFIG_HOME="$CFG" \
BIOROUTER_DISABLE_KEYRING=true \
BIOROUTER_UPDATE_FEED_URL="http://127.0.0.1:$PORT" \
BIOROUTER_UPDATE_AUTO_INSTALL=1 \
ENABLE_DEV_UPDATES=true \
  "$BIN" --user-data-dir="$USERDATA" >"$WORK/stdout.log" 2>&1 &
APP_PID=$!

echo "==> polling for in-place swap to $NEW_VER (up to ~150s)"
RESULT="timeout"
for i in $(seq 1 30); do
  sleep 5
  NOW="$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$PLIST" 2>/dev/null || echo '?')"
  echo "    [$((i*5))s] on-disk version = $NOW"
  if [ "$NOW" = "$NEW_VER" ]; then RESULT="swapped"; break; fi
done

echo "==> updater log excerpt:"
grep -iE "feed override|auto-updater|update-(available|downloaded)|checking|squirrel|quitAndInstall|download-progress|AUTO_INSTALL|error" "$APPLOG" 2>/dev/null | tail -30 | sed 's/^/      /'
[ -s "$APPLOG" ] || echo "      (no electron-log produced — app may not have started; stdout below)"
[ -s "$WORK/stdout.log" ] && { echo "      --- stdout ---"; tail -15 "$WORK/stdout.log" | sed 's/^/      /'; }

AFTER="$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$PLIST" 2>/dev/null || echo '?')"
echo
echo "==================== RESULT ===================="
echo "  before: $BEFORE   after: $AFTER   outcome: $RESULT"
if [ "$RESULT" = "swapped" ] && [ "$AFTER" = "$NEW_VER" ]; then
  echo "  PASS: notarized in-place auto-update swap succeeded ($BEFORE -> $AFTER)"
  exit 0
fi
echo "  FAIL: app did not swap to $NEW_VER"
echo "  (full app log at $WORK/app.log — kept for inspection)"
trap - EXIT; cleanup
exit 1
