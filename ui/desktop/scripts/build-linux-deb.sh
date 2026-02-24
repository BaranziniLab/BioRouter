#!/usr/bin/env bash
# Runs INSIDE a linux/amd64 Docker container to produce the Linux .deb package.
# Called by: just make-ui-linux (step 2/2)
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DESKTOP_DIR="$(dirname "$SCRIPT_DIR")"
UI_DIR="$(dirname "$DESKTOP_DIR")"
PROJECT_ROOT="$(dirname "$UI_DIR")"
BIN_DIR="$DESKTOP_DIR/src/bin"
LINUX_RELEASE="$PROJECT_ROOT/target/x86_64-unknown-linux-gnu/release"

echo "Installing system dependencies (fakeroot, dpkg, rpm)..."
apt-get update -q
apt-get install -y --no-install-recommends fakeroot dpkg rpm

echo "Replacing macOS ARM binaries with Linux x64 binaries in src/bin/..."
# Remove macOS ARM executables — they are non-functional on Linux
for f in biorouterd biorouter jbang npx uvx node; do
    if [ -f "$BIN_DIR/$f" ]; then
        rm -f "$BIN_DIR/$f"
        echo "  Removed macOS binary: $f"
    fi
done

# Copy Linux x64 Rust binaries
if [ ! -f "$LINUX_RELEASE/biorouter" ]; then
    echo "ERROR: Linux binaries not found at $LINUX_RELEASE"
    echo "       Run step 1 (Rust cross-compilation) before this step."
    exit 1
fi
cp "$LINUX_RELEASE/biorouter" "$BIN_DIR/biorouter"
chmod +x "$BIN_DIR/biorouter"
echo "  Installed Linux x64: biorouter"

echo "Running electron-forge make for Linux x64 (deb, rpm, zip)..."
cd "$DESKTOP_DIR"
npm ci --cache /root/.npm
ELECTRON_PLATFORM=linux ELECTRON_ARCH=x64 npm run make -- --platform=linux --arch=x64 \
    --targets "@electron-forge/maker-deb,@electron-forge/maker-rpm"

echo ""
echo "Done! .deb package is at:"
echo "  $DESKTOP_DIR/out/make/deb/x64/"
