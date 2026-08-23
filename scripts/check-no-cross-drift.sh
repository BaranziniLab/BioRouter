#!/usr/bin/env bash
# BR-70: fail if a docker cross-compile recipe drifts from scripts/cross-env.sh.
#
# The cross-compile invariants — the pinned base images (above all the linux
# glibc-2.31 floor), the mingw winpthread linker wrap, and LZMA_API_STATIC=1 —
# must live in exactly ONE place: scripts/cross-env.sh. A second copy is how the
# Justfile quietly ended up on `rust:latest` (glibc 2.39), which produces linux
# binaries that will not start on Debian 12 / Ubuntu 22.04 / Rocky 9. This guard
# is the tripwire; it runs in `just check-everything` and in CI.
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0
err() { printf '::error::%s\n' "$1" >&2; fail=1; }

# Tokens that only ever appear in an actual cross-compile RECIPE — never in a
# mere path reference like `target/x86_64-.../release/biorouterd`. If any show up
# outside the owners below, someone forked the recipe.
tokens='rustup target add x86_64-(pc-windows-gnu|unknown-linux-gnu)|winpthread-gcc|LZMA_API_STATIC|CARGO_TARGET_X86_64_(PC_WINDOWS_GNU|UNKNOWN_LINUX_GNU)_LINKER'

# Owners exempt from the recipe-token scan:
#   scripts/cross-env.sh             — THE one true recipe.
#   scripts/check-no-cross-drift.sh  — this file (it names the tokens).
#
# There used to be a second exempt recipe, scripts/build-headless-linux.sh, for
# the standalone browser binary. That binary is retired, so there is exactly ONE
# cross recipe again and nothing else may be exempted without a reason written
# here.
exempt='scripts/cross-env\.sh|scripts/check-no-cross-drift\.sh'

# Drop comment lines (a comment may legitimately name a token) and the exempt
# owners, then report anything left.
hits=$(grep -rHnE "$tokens" Justfile scripts/ .github/ 2>/dev/null \
        | grep -vE "$exempt" \
        | awk '{ c=$0; sub(/^[^:]*:[0-9]+:/, "", c); if (c !~ /^[[:space:]]*#/) print }' \
        || true)
if [ -n "$hits" ]; then
  err "cross-compile recipe found outside scripts/cross-env.sh:"
  printf '%s\n' "$hits" >&2
fi

# The glibc floor must never be raised by switching to a rolling image.
grep -q 'LINUX_RUST_IMG:=rust:1.92-bullseye' scripts/cross-env.sh \
  || err "LINUX_RUST_IMG drifted off the bullseye (glibc 2.31) pin in scripts/cross-env.sh"

# And no second recipe may reappear by being quietly re-exempted: the exemption
# list above must name only cross-env.sh and this file. Asserted rather than
# trusted, because a check that greps a file which no longer exists still exits
# 0 -- it simply stops checking, and says so only on stderr.
for owner in scripts/cross-env.sh scripts/check-no-cross-drift.sh; do
  [ -f "$owner" ] || err "exempt owner $owner is missing; this guard is checking less than it claims"
done

[ "$fail" = 0 ] || exit 1
echo "OK — one cross recipe, glibc floor intact."
