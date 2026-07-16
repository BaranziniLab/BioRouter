#!/usr/bin/env bash
# Execute the shipped vX.Y.Z artifacts in their target environments.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESK="$ROOT/ui/desktop"
VERSION="${1:?usage: scripts/smoke-test-release-artifacts.sh <version> [all|mac|deb|rpm|cli|headless]}"
TARGET="${2:-all}"

log() { printf '\033[1;36m[release-smoke]\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m[release-smoke] %s\033[0m\n' "$*" >&2; exit 1; }

require_file() {
  [ -f "$1" ] || die "missing artifact: $1"
}

smoke_mac() {
  local arch="$1" dmg="$2" mount tmp runner=(/usr/bin/env)
  require_file "$dmg"
  mount="$(mktemp -d "/tmp/biorouter-${arch}-mount.XXXXXX")"
  tmp="$(mktemp -d "/tmp/biorouter-${arch}-smoke.XXXXXX")"
  hdiutil attach -nobrowse -readonly -mountpoint "$mount" "$dmg" >/dev/null
  local app="$mount/Biorouter.app"
  [ -d "$app" ] || die "$arch DMG does not contain Biorouter.app"
  codesign --verify --deep --strict --verbose=2 "$app"
  spctl --assess --type execute --verbose "$app"
  xcrun stapler validate "$app"
  if [ "$arch" = x64 ]; then
    arch -x86_64 /usr/bin/true >/dev/null 2>&1 || die "Rosetta is required for Intel runtime verification"
    runner=(arch -x86_64)
  fi
  file "$app/Contents/Resources/bin/biorouter" | grep -q "${arch/x64/x86_64}" \
    || die "$arch CLI architecture mismatch"
  "${runner[@]}" "$app/Contents/Resources/bin/biorouter" --version | grep -q "$VERSION"
  "${runner[@]}" "$app/Contents/Resources/bin/biorouterd" --version | grep -q "$VERSION"
  HOME="$tmp" BIOROUTER_DISABLE_KEYRING=true \
    "${runner[@]}" "$app/Contents/MacOS/Biorouter" --disable-gpu >"$tmp/app.log" 2>&1 &
  local pid=$!
  sleep 12
  kill -0 "$pid" 2>/dev/null || { sed -n '1,160p' "$tmp/app.log" >&2; die "$arch desktop exited during startup"; }
  kill "$pid" 2>/dev/null || true
  sleep 1
  kill -KILL "$pid" 2>/dev/null || true
  pkill -KILL -f "$mount/Biorouter.app/Contents/" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  hdiutil detach "$mount" >/dev/null 2>&1 || {
    sleep 2
    hdiutil detach -force "$mount" >/dev/null
  }
  rm -rf "$mount" "$tmp"
  log "macOS $arch DMG, CLI, daemon, and desktop startup passed"
}

smoke_deb() {
  local deb="$DESK/out/make/deb/x64/biorouter_${VERSION}_amd64.deb"
  require_file "$deb"
  docker run --rm --platform linux/amd64 -e VERSION="$VERSION" \
    -v "$deb":/pkg/biorouter.deb:ro debian:bookworm-slim bash -euxc '
      apt-get update -qq
      apt-get install -y -qq /pkg/biorouter.deb xvfb >/dev/null
      /usr/lib/biorouter/resources/bin/biorouter --version | grep -q "$VERSION"
      /usr/lib/biorouter/resources/bin/biorouterd --version | grep -q "$VERSION"
      HOME=/tmp/biorouter-home BIOROUTER_DISABLE_KEYRING=true \
        xvfb-run -a /usr/bin/biorouter --no-sandbox >/tmp/biorouter.log 2>&1 &
      pid=$!
      sleep 12
      kill -0 "$pid"
      kill "$pid" || true
      sleep 1
      kill -KILL "$pid" || true
      wait "$pid" || true
    '
  log "Linux desktop DEB, CLI, daemon, and Xvfb startup passed"
}

smoke_rpm() {
  local rpm="$DESK/out/make/rpm/x64/Biorouter-${VERSION}-1.x86_64.rpm"
  require_file "$rpm"
  docker run --rm --platform linux/amd64 -e VERSION="$VERSION" \
    -v "$rpm":/pkg/biorouter.rpm:ro rockylinux:9 bash -euxc '
      dnf install -y -q /pkg/biorouter.rpm xorg-x11-server-Xvfb >/dev/null
      /usr/lib/Biorouter/resources/bin/biorouter --version | grep -q "$VERSION"
      /usr/lib/Biorouter/resources/bin/biorouterd --version | grep -q "$VERSION"
      Xvfb :99 -screen 0 1280x800x24 >/tmp/xvfb.log 2>&1 &
      xvfb=$!
      export DISPLAY=:99
      HOME=/tmp/biorouter-home BIOROUTER_DISABLE_KEYRING=true \
        /usr/bin/Biorouter --no-sandbox >/tmp/biorouter.log 2>&1 &
      pid=$!
      sleep 12
      kill -0 "$pid"
      kill "$pid" "$xvfb" || true
      sleep 1
      kill -KILL "$pid" "$xvfb" || true
      wait "$pid" || true
    '
  log "Linux desktop RPM, CLI, daemon, and Xvfb startup passed"
}

smoke_cli_packages() {
  local deb="$ROOT/dist/cli/biorouter-cli_${VERSION}_amd64.deb"
  local rpm="$ROOT/dist/cli/biorouter-cli-${VERSION}-1.x86_64.rpm"
  require_file "$deb"
  require_file "$rpm"
  docker run --rm --platform linux/amd64 -e VERSION="$VERSION" \
    -v "$deb":/pkg/biorouter-cli.deb:ro debian:bookworm-slim bash -euxc '
      apt-get update -qq
      apt-get install -y -qq /pkg/biorouter-cli.deb >/dev/null
      biorouter --version | grep -q "$VERSION"
      biorouterd --version | grep -q "$VERSION"
      biorouter term --help >/dev/null
    '
  docker run --rm --platform linux/amd64 -e VERSION="$VERSION" \
    -v "$rpm":/pkg/biorouter-cli.rpm:ro rockylinux:9 bash -euxc '
      dnf install -y -q /pkg/biorouter-cli.rpm >/dev/null
      biorouter --version | grep -q "$VERSION"
      biorouterd --version | grep -q "$VERSION"
      biorouter term --help >/dev/null
    '
  log "CLI-only DEB/RPM version and terminal entry points passed"
}

smoke_headless() {
  local tarball="$ROOT/dist/biorouter-headless-linux-x64.tar.gz"
  require_file "$tarball"
  docker run --rm --platform linux/amd64 -e VERSION="$VERSION" \
    -v "$tarball":/pkg/biorouter-headless.tar.gz:ro debian:bookworm-slim bash -euxc '
      apt-get update -qq
      apt-get install -y -qq curl ca-certificates xvfb xauth >/dev/null
      mkdir -p /app
      tar -xzf /pkg/biorouter-headless.tar.gz -C /app
      install=/app/headless-linux-x64
      "$install/bin/biorouter" --version | grep -q "$VERSION"
      "$install/bin/biorouterd" --version | grep -q "$VERSION"
      "$install/bin/biorouter-headless" --version | grep -q "$VERSION"
      HOME=/tmp/biorouter-home "$install/bin/biorouter-headless" serve \
        --no-spawn --host 127.0.0.1 --port 18080 --web-dir "$install/web" >/tmp/headless.log 2>&1 &
      pid=$!
      for _ in $(seq 1 30); do
        curl -fsS http://127.0.0.1:18080/headless/health >/tmp/health.json && break
        sleep 1
      done
      grep -q "\"status\":\"ok\"" /tmp/health.json
      curl -fsS http://127.0.0.1:18080/ | grep -qi "<!doctype html>"
      kill "$pid"
      wait "$pid" || true
    '
  log "headless tarball binaries, health endpoint, and web app passed"
}

case "$TARGET" in
  all)
    smoke_mac arm64 "$DESK/out/make/Biorouter-${VERSION}-arm64.dmg"
    smoke_mac x64 "$DESK/out/make/Biorouter-${VERSION}-x64.dmg"
    smoke_deb
    smoke_rpm
    smoke_cli_packages
    smoke_headless
    log "all locally executable release artifacts passed"
    ;;
  mac)
    smoke_mac arm64 "$DESK/out/make/Biorouter-${VERSION}-arm64.dmg"
    smoke_mac x64 "$DESK/out/make/Biorouter-${VERSION}-x64.dmg"
    ;;
  deb) smoke_deb ;;
  rpm) smoke_rpm ;;
  cli) smoke_cli_packages ;;
  headless) smoke_headless ;;
  *) die "unknown smoke target: $TARGET" ;;
esac
