#!/usr/bin/env bash
# Build a headless Linux x64 artifact from this checkout:
#   dist/headless-linux-x64/bin/{biorouter,biorouterd,biorouter-headless}
#   dist/headless-linux-x64/web/
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Inherit the glibc-floor pin ($LINUX_RUST_IMG = rust:1.92-bullseye) from the one
# source of truth. This is a SPECIALIZED cross recipe — the headless browser
# binary links extra system libs the 2-binary release build never needs — so it
# does not use cross_linux, but it must never fork the floor. (check-no-cross-drift
# enforces that this image comes from the shared pin.)
# shellcheck source=scripts/cross-env.sh
. "$ROOT/scripts/cross-env.sh"
OUT="$ROOT/dist/headless-linux-x64"
TARGET_DIR="${BIOROUTER_HEADLESS_TARGET_DIR:-/tmp/biorouter-headless-bullseye-target}"
RUST_IMAGE="${BIOROUTER_HEADLESS_RUST_IMAGE:-$LINUX_RUST_IMG}"
HOST_UID="$(id -u)"
HOST_GID="$(id -g)"

log() { printf '\033[1;36m[headless]\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m[headless] %s\033[0m\n' "$*" >&2; exit 1; }
BR_HINT_LABEL="headless"
# shellcheck source=scripts/lib/dependency-hint.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/dependency-hint.sh"


br_require_command docker "The Linux backend is cross-compiled inside a container."
docker info >/dev/null 2>&1 || br_dependency_die docker "docker daemon is not running" \
  "The docker CLI is installed but cannot reach a daemon. Start Docker Desktop (or dockerd) and retry."
br_require_command npm "Needed for the browser bundle. Activate hermit first: source bin/activate-hermit"

cd "$ROOT"
mkdir -p "$OUT/bin"

log "building Linux x64 Rust binaries in Docker"
docker run --rm \
  -e HOST_UID="$HOST_UID" \
  -e HOST_GID="$HOST_GID" \
  -v "$ROOT":/work \
  -v "$TARGET_DIR":"$TARGET_DIR" \
  -w /work \
  "$RUST_IMAGE" \
  bash -euxo pipefail -c '
    dpkg --add-architecture amd64
    apt-get update -q
    apt-get install -y --no-install-recommends \
      ca-certificates curl pkg-config protobuf-compiler cmake make clang \
      gcc-x86-64-linux-gnu g++-x86-64-linux-gnu \
      libc6-dev-amd64-cross linux-libc-dev-amd64-cross \
      libssl-dev:amd64 liblzma-dev:amd64 libbz2-dev:amd64 zlib1g-dev:amd64 \
      libsqlite3-dev:amd64 libdbus-1-dev:amd64 \
      libxcb1-dev:amd64 libxcb-render0-dev:amd64 libxcb-shape0-dev:amd64 \
      libxcb-xfixes0-dev:amd64 libfontconfig1-dev:amd64 libfreetype-dev:amd64 \
      libexpat1-dev:amd64
    if ! command -v rustup >/dev/null 2>&1; then
      curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    fi
    . /usr/local/cargo/env
    rustup default 1.92
    rustup target add x86_64-unknown-linux-gnu
    export CC_x86_64_unknown_linux_gnu=x86_64-linux-gnu-gcc
    export CXX_x86_64_unknown_linux_gnu=x86_64-linux-gnu-g++
    export AR_x86_64_unknown_linux_gnu=x86_64-linux-gnu-ar
    export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc
    export PKG_CONFIG_ALLOW_CROSS=1
    export PKG_CONFIG_PATH_x86_64_unknown_linux_gnu=/usr/lib/x86_64-linux-gnu/pkgconfig
    export PROTOC=/usr/bin/protoc
    export LZMA_API_STATIC=1
    cargo build --release \
      --target x86_64-unknown-linux-gnu \
      --target-dir '"$TARGET_DIR"' \
      --bin biorouterd \
      --bin biorouter \
      --bin biorouter-headless
    mkdir -p /work/dist/headless-linux-x64/bin
    cp '"$TARGET_DIR"'/x86_64-unknown-linux-gnu/release/biorouterd /work/dist/headless-linux-x64/bin/
    cp '"$TARGET_DIR"'/x86_64-unknown-linux-gnu/release/biorouter /work/dist/headless-linux-x64/bin/
    cp '"$TARGET_DIR"'/x86_64-unknown-linux-gnu/release/biorouter-headless /work/dist/headless-linux-x64/bin/
    chown -R "$HOST_UID:$HOST_GID" /work/dist/headless-linux-x64/bin
  '

log "building browser bundle"
(
  cd "$ROOT/ui/desktop"
  npm run generate-api
  npx vite build --config vite.renderer.config.mts --outDir "$OUT/web" --emptyOutDir
)

log "writing manifest"
(
  cd "$OUT"
  {
    printf 'created_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'git_commit=%s\n' "$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || true)"
    shasum -a 256 bin/biorouter bin/biorouterd bin/biorouter-headless
  } > manifest.txt
)

log "artifact ready at $OUT"
