#!/usr/bin/env bash
# Build the CLI-only Linux packages (.deb + .rpm) — just the headless
# `biorouter` CLI and `biorouterd` daemon, no Electron/GUI.
#
# Prereqs: the linux-gnu binaries must already exist (built by
# `scripts/release.sh backends <ver>`):
#   target/x86_64-unknown-linux-gnu/release/{biorouter,biorouterd}
#
# Usage: scripts/build-cli-linux-packages.sh <version>
# Output:
#   dist/cli/biorouter-cli_<version>_amd64.deb
#   dist/cli/biorouter-cli-<version>-1.x86_64.rpm
#
# Each package is then smoke-tested in a clean container (install + run
# `biorouter --version` and `biorouter doctor`) so we only ship functional
# artifacts.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VERSION="${1:?usage: build-cli-linux-packages.sh <version>}"
REL="target/x86_64-unknown-linux-gnu/release"
OUT="dist/cli"
DEB="$OUT/biorouter-cli_${VERSION}_amd64.deb"
RPM="$OUT/biorouter-cli-${VERSION}-1.x86_64.rpm"
NFPM_IMAGE="goreleaser/nfpm:latest"

log() { printf '\033[1;36m[cli-pkg]\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m[cli-pkg] %s\033[0m\n' "$*" >&2; exit 1; }

command -v docker >/dev/null 2>&1 || die "docker is required"
docker info >/dev/null 2>&1 || die "docker daemon is not running"
[ -f "$REL/biorouter" ]  || die "missing $REL/biorouter — run: scripts/release.sh backends $VERSION"
[ -f "$REL/biorouterd" ] || die "missing $REL/biorouterd — run: scripts/release.sh backends $VERSION"

mkdir -p "$OUT"
rm -f "$DEB" "$RPM"

# ── 1. Build deb + rpm with nfpm (no root needed) ─────────────────────────────
log "building deb + rpm with nfpm ($VERSION)"
docker run --rm -e VERSION="$VERSION" -v "$ROOT":/work -w /work "$NFPM_IMAGE" \
  package -f packaging/cli/nfpm.yaml -p deb -t "$DEB"
docker run --rm -e VERSION="$VERSION" -v "$ROOT":/work -w /work "$NFPM_IMAGE" \
  package -f packaging/cli/nfpm.yaml -p rpm -t "$RPM"

[ -f "$DEB" ] || die "deb was not produced"
[ -f "$RPM" ] || die "rpm was not produced"
log "deb: $DEB ($(du -h "$DEB" | cut -f1))"
log "rpm: $RPM ($(du -h "$RPM" | cut -f1))"

# ── 2. Smoke-test the .deb on a clean Debian system ───────────────────────────
log "smoke-testing .deb on debian:bookworm-slim"
docker run --rm --platform linux/amd64 -v "$ROOT/$OUT":/pkg debian:bookworm-slim bash -euxc '
  apt-get update -q
  apt-get install -y --no-install-recommends "/pkg/'"$(basename "$DEB")"'"
  which biorouter biorouterd
  biorouter --version
  biorouterd --version
  biorouter doctor --no-update --format json >/dev/null
  echo "DEB_SMOKE_OK"
' | grep -q DEB_SMOKE_OK || die "deb smoke test FAILED"
log "deb smoke test passed ✓"

# ── 3. Smoke-test the .rpm on a clean Rocky Linux system ──────────────────────
log "smoke-testing .rpm on rockylinux:9"
docker run --rm --platform linux/amd64 -v "$ROOT/$OUT":/pkg rockylinux:9 bash -euxc '
  dnf install -y "/pkg/'"$(basename "$RPM")"'"
  which biorouter biorouterd
  biorouter --version
  biorouterd --version
  biorouter doctor --no-update --format json >/dev/null
  echo "RPM_SMOKE_OK"
' | grep -q RPM_SMOKE_OK || die "rpm smoke test FAILED"
log "rpm smoke test passed ✓"

log "CLI-only Linux packages built and verified:"
log "  $DEB"
log "  $RPM"
