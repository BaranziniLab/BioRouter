#!/usr/bin/env bash
# BR-70: assert the cross-built linux binaries do not require a glibc newer than
# the floor (default 2.31 / Debian 11 bullseye). A `cargo check` cannot see this
# — checks do not link, so no symbol versions are emitted — so this runs after
# the nightly full cross BUILD (and locally before a release). It is the check
# that would have MECHANICALLY caught the `rust:latest` (trixie, glibc 2.39)
# regression the release comments describe, instead of a human noticing a
# container smoke test failed.
#
# Usage: scripts/check-glibc-floor.sh [BIN_DIR]
#   BIN_DIR defaults to target/x86_64-unknown-linux-gnu/release
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/cross-env.sh
. "$(dirname "$0")/cross-env.sh"

BIN_DIR="${1:-target/x86_64-unknown-linux-gnu/release}"
for b in biorouterd biorouter; do
  [ -f "$BIN_DIR/$b" ] || { echo "::error::missing binary $BIN_DIR/$b — run the cross build first"; exit 2; }
done

# objdump lives in the pinned cross image; run it there so the host needs no
# binutils. Extract every GLIBC_x.y symbol version the binaries import and keep
# the highest.
worst=$(docker run --rm -v "$PWD":/w -w /w "$LINUX_RUST_IMG" sh -c "
    objdump -T $BIN_DIR/biorouterd $BIN_DIR/biorouter 2>/dev/null \
      | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -V | tail -1")

[ -n "$worst" ] || { echo "::error::could not read glibc symbol versions from $BIN_DIR"; exit 2; }

if [ "$(printf '%s\n%s\n' "$GLIBC_FLOOR" "$worst" | sort -V | tail -1)" != "$GLIBC_FLOOR" ]; then
  echo "::error::binaries require glibc $worst > floor $GLIBC_FLOOR — Debian 12 / Ubuntu 22.04 / Rocky 9 would fail to start"
  exit 1
fi
echo "OK — max glibc requirement $worst ≤ floor $GLIBC_FLOOR"
