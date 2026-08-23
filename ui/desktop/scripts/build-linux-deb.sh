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
# Remove Windows executables and DLLs — they do not belong in Linux packages
for f in "$BIN_DIR"/*.exe "$BIN_DIR"/*.dll "$BIN_DIR"/*.cmd; do
    if [ -f "$f" ]; then
        rm -f "$f"
        echo "  Removed Windows file: $(basename "$f")"
    fi
done
# Remove bundled MinGit (Windows-only Git distribution dropped by download-mingit.js)
if [ -d "$BIN_DIR/git" ]; then
    rm -rf "$BIN_DIR/git"
    echo "  Removed MinGit directory: git/"
fi

# Copy Linux x64 Rust binaries
if [ ! -f "$LINUX_RELEASE/biorouter" ] || [ ! -f "$LINUX_RELEASE/biorouterd" ]; then
    echo "ERROR: Linux binaries not found at $LINUX_RELEASE"
    echo "       Run step 1 (Rust cross-compilation) before this step."
    exit 1
fi
cp "$LINUX_RELEASE/biorouter" "$BIN_DIR/biorouter"
chmod +x "$BIN_DIR/biorouter"
echo "  Installed Linux x64: biorouter"
cp "$LINUX_RELEASE/biorouterd" "$BIN_DIR/biorouterd"
chmod +x "$BIN_DIR/biorouterd"
echo "  Installed Linux x64: biorouterd"

cd "$DESKTOP_DIR"
npm ci --cache /root/.npm

# The browser interface bundle biorouterd serves (BIOROUTER_SERVE_UI), shipped
# as the extraResource 'src/web'. Built explicitly HERE because this script
# calls `npm run make` directly and so never runs prepare-platform-binaries.js,
# which is where every other platform's packaging path builds it.
#
# The deb and rpm install under /opt, so `<exe dir>/../web` resolves inside the
# app tree exactly as it does on macOS and Windows. It is the CLI-only packages
# (packaging/cli/nfpm.yaml, /usr/bin) that cannot use that rule and place the
# bundle at /usr/share/biorouter/web instead.
echo "Building browser interface bundle (npm run build:web)..."
npm run build:web
[ -s "$DESKTOP_DIR/src/web/index.html" ] || {
    echo "ERROR: npm run build:web produced no src/web/index.html"
    exit 1
}

echo "Running electron-forge make for Linux x64 (deb, rpm, zip)..."
ELECTRON_PLATFORM=linux ELECTRON_ARCH=x64 npm run make -- --platform=linux --arch=x64 \
    --targets "@electron-forge/maker-deb,@electron-forge/maker-rpm"

echo ""
echo "Done! .deb package is at:"
echo "  $DESKTOP_DIR/out/make/deb/x64/"
