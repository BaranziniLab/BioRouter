#!/usr/bin/env bash
# Build a headless Linux x64 artifact from this checkout:
#   dist/headless-linux-x64/bin/{biorouter,biorouterd,biorouter-headless}
#   dist/headless-linux-x64/web/
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/dist/headless-linux-x64"
TARGET_DIR="${BIOROUTER_HEADLESS_TARGET_DIR:-/tmp/biorouter-target}"
RUST_IMAGE="${BIOROUTER_HEADLESS_RUST_IMAGE:-rust:1.92}"
HOST_UID="$(id -u)"
HOST_GID="$(id -g)"

log() { printf '\033[1;36m[headless]\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m[headless] %s\033[0m\n' "$*" >&2; exit 1; }

command -v docker >/dev/null 2>&1 || die "docker is required"
docker info >/dev/null 2>&1 || die "docker daemon is not running"
command -v npm >/dev/null 2>&1 || die "npm is required for the browser bundle"

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
