# BioRouter Build Guide

This guide covers how to build BioRouter for all supported platforms from a macOS Apple Silicon development machine. Follow the steps in order — some steps depend on previous ones.

---

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Step 1 — Build the Rust Binary (macOS ARM64)](#step-1--build-the-rust-binary-macos-arm64)
3. [Step 2 — Build macOS Apple Silicon App (Signed + Notarized)](#step-2--build-macos-apple-silicon-app-signed--notarized)
4. [Step 3 — Build macOS Intel App (Signed + Notarized)](#step-3--build-macos-intel-app-signed--notarized)
5. [Step 4 — Build Linux App via Docker (.deb + .rpm)](#step-4--build-linux-app-via-docker-deb--rpm)
6. [Step 5 — Restore macOS ARM Binary After Linux Build](#step-5--restore-macos-arm-binary-after-linux-build)
7. [Step 6 — Build Windows App](#step-6--build-windows-app)
8. [Output Artifacts](#output-artifacts)
9. [One-Time Setup](#one-time-setup)
10. [Troubleshooting](#troubleshooting)

---

## Prerequisites

Before building, ensure the following are installed and configured:

| Tool | Install |
|------|---------|
| **Rust** (with `cargo`) | https://rustup.rs |
| **Node.js** v24+ | https://nodejs.org |
| **npm** | bundled with Node.js |
| **Docker Desktop** | https://www.docker.com/products/docker-desktop (required for Linux build) |
| **Xcode Command Line Tools** | `xcode-select --install` |

Also required for macOS signed builds — see [One-Time Setup](#one-time-setup):
- Apple Developer certificate imported into your keychain
- Apple app-specific password (from appleid.apple.com)

All commands below assume you start from the repo root:
```bash
cd /path/to/BioRouter
```

---

## Step 1 — Build the Rust Binary (macOS ARM64)

This compiles the native macOS ARM64 backend binary. It must be done before any Electron build.

```bash
cargo build --release
```

Output: `target/release/biorouter`

Then copy it into the Electron app's bin directory:

```bash
cp target/release/biorouter ui/desktop/src/bin/biorouter
```

Verify it is the right architecture:
```bash
file ui/desktop/src/bin/biorouter
# Expected: Mach-O 64-bit executable arm64
```

> **Note:** The ARM64 binary is gitignored (it exceeds GitHub's 100MB limit). It must be rebuilt on each machine.

---

## Step 2 — Build macOS Apple Silicon App (Signed + Notarized)

Requires the ARM64 binary from Step 1 to be in `ui/desktop/src/bin/biorouter`.

```bash
cd ui/desktop

APPLE_ID=wanjungu001@gmail.com \
APPLE_APP_SPECIFIC_PASSWORD=rznx-yhkv-ukxx-ycug \
npm run bundle:default
```

What this does:
1. Strips Windows binaries from `src/bin/` via `prepare-platform-binaries.js`
2. Builds and packages the Electron app for `darwin/arm64`
3. Signs all binaries with the Developer ID certificate
4. Submits to Apple's notarization service and staples the ticket
5. Creates the final distributable zip with `ditto`

**Verify notarization:**
```bash
spctl --assess --verbose out/BioRouter-darwin-arm64/BioRouter.app
# Expected: accepted  source=Notarized Developer ID
```

Output: `out/BioRouter-darwin-arm64/BioRouter.zip`

---

## Step 3 — Build macOS Intel App (Signed + Notarized)

Intel Mac cross-compilation requires the `x86_64-apple-darwin` Rust target. Install it once:

```bash
rustup target add x86_64-apple-darwin
```

Then cross-compile the Rust binary for Intel:

```bash
cargo build --release --target x86_64-apple-darwin
```

Output: `target/x86_64-apple-darwin/release/biorouter`

Copy the Intel binary into the bin directory (replacing ARM):

```bash
cp target/x86_64-apple-darwin/release/biorouter ui/desktop/src/bin/biorouter
```

Verify:
```bash
file ui/desktop/src/bin/biorouter
# Expected: Mach-O 64-bit executable x86_64
```

Now build the Intel Electron app:

```bash
cd ui/desktop

APPLE_ID=wanjungu001@gmail.com \
APPLE_APP_SPECIFIC_PASSWORD=rznx-yhkv-ukxx-ycug \
npm run bundle:intel
```

**Verify notarization:**
```bash
spctl --assess --verbose out/BioRouter-darwin-x64/BioRouter.app
# Expected: accepted  source=Notarized Developer ID
```

Output: `out/BioRouter-darwin-x64/BioRouter_intel_mac.zip`

After this step, restore the ARM binary so subsequent builds aren't broken:

```bash
cp target/release/biorouter ui/desktop/src/bin/biorouter
```

---

## Step 4 — Build Linux App via Docker (.deb + .rpm)

The Linux build runs entirely inside Docker — no Linux machine needed. Docker Desktop must be running.

This is a two-stage process.

### Stage A — Cross-compile Rust for Linux x64

```bash
docker run --rm \
  --platform linux/amd64 \
  -v "$(pwd):/workspace" \
  -v "biorouter-linux-cache:/root/.cargo/registry" \
  -w /workspace \
  rust:latest \
  bash -c "
    rustup target add x86_64-unknown-linux-gnu && \
    dpkg --add-architecture amd64 && \
    apt-get update -q && \
    apt-get install -y --no-install-recommends \
      gcc-x86-64-linux-gnu g++-x86-64-linux-gnu \
      protobuf-compiler cmake \
      libxcb1-dev:amd64 libbz2-dev:amd64 && \
    export CC_x86_64_unknown_linux_gnu=x86_64-linux-gnu-gcc && \
    export CXX_x86_64_unknown_linux_gnu=x86_64-linux-gnu-g++ && \
    export AR_x86_64_unknown_linux_gnu=x86_64-linux-gnu-ar && \
    export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc && \
    export PKG_CONFIG_ALLOW_CROSS=1 && \
    export PKG_CONFIG_PATH_x86_64_unknown_linux_gnu=/usr/lib/x86_64-linux-gnu/pkgconfig && \
    export PROTOC=/usr/bin/protoc && \
    cargo build --release --target x86_64-unknown-linux-gnu
  "
```

The `biorouter-linux-cache` Docker volume caches the Cargo registry between runs to speed up subsequent builds. On the first run this will take ~5-10 minutes; subsequent runs are much faster.

Verify the Linux binary was produced:
```bash
ls -lh target/x86_64-unknown-linux-gnu/release/biorouter
# Expected: ~100-115MB ELF 64-bit binary
```

### Stage B — Package into .deb and .rpm

```bash
docker run --rm \
  --platform linux/amd64 \
  -v "$(pwd):/workspace" \
  -v "biorouter-linux-npm-cache:/root/.npm" \
  -w /workspace/ui/desktop \
  node:20-bookworm \
  bash /workspace/ui/desktop/scripts/build-linux-deb.sh
```

The `build-linux-deb.sh` script:
1. Installs `fakeroot`, `dpkg`, and `rpm` inside the container
2. Swaps the macOS ARM binary in `src/bin/` with the Linux x64 binary
3. Runs `electron-forge make` for `linux/x64` producing `.deb` and `.rpm`

Outputs:
- `ui/desktop/out/make/deb/x64/biorouter_1.20.0_amd64.deb`
- `ui/desktop/out/make/rpm/x64/BioRouter-1.20.0-1.x86_64.rpm`

---

## Step 5 — Restore macOS ARM Binary After Linux Build

The Linux Docker build replaces `src/bin/biorouter` with the Linux x64 binary. **Always restore the ARM binary before the Windows build or any future macOS work:**

```bash
cp target/release/biorouter ui/desktop/src/bin/biorouter
```

Also, the Docker `npm ci` inside the container corrupts the local `node_modules` (missing macOS ARM native modules). Fix this every time after a Linux Docker build:

```bash
cd ui/desktop
rm -rf node_modules package-lock.json
npm install
cd ../..
```

---

## Step 6 — Build Windows App

Requires the macOS ARM binary to be restored (Step 5). The Windows Rust binary (`biorouter.exe`) is pre-built and stored in `ui/desktop/src/platform/windows/bin/` — the build script copies it automatically.

```bash
cd ui/desktop
npm run bundle:windows
```

What this does:
1. Builds the Vite main process bundle
2. Copies Windows-specific binaries (`.exe`, `.dll`, `.cmd`) from `src/platform/windows/bin/` into `src/bin/`
3. Removes the macOS binary from `src/bin/`
4. Runs `electron-forge make` for `win32/x64`

Output: `out/make/zip/win32/x64/BioRouter-win32-x64-1.20.0.zip`

After this, restore the ARM binary again:
```bash
cp target/release/biorouter ui/desktop/src/bin/biorouter
```

---

## Output Artifacts

After completing all steps, your distributable files are:

| Platform | File | Location |
|----------|------|----------|
| macOS Apple Silicon | `BioRouter.zip` | `ui/desktop/out/BioRouter-darwin-arm64/` |
| macOS Intel | `BioRouter_intel_mac.zip` | `ui/desktop/out/BioRouter-darwin-x64/` |
| Linux Ubuntu / Pop!_OS | `biorouter_1.20.0_amd64.deb` | `ui/desktop/out/make/deb/x64/` |
| Linux Fedora / RHEL | `BioRouter-1.20.0-1.x86_64.rpm` | `ui/desktop/out/make/rpm/x64/` |
| Windows x64 | `BioRouter-win32-x64-1.20.0.zip` | `ui/desktop/out/make/zip/win32/x64/` |

Upload all five files to the GitHub Release assets.

---

## One-Time Setup

These steps only need to be done once per development machine.

### Import the Apple Developer Certificate

The `.p12` file and full instructions are in `APPLE_DEVELOPER_NOTES.md` (gitignored, keep local).

```bash
security import UCSF-AppleDeveloper-Main_Application.p12 \
  -k ~/Library/Keychains/login.keychain-db \
  -P "92nfes0aefuwe" \
  -T /usr/bin/codesign \
  -T /usr/bin/productbuild
```

### Install the Apple Developer ID G2 Intermediate Certificate

Required for a complete trust chain:

```bash
curl -s -o /tmp/DeveloperIDG2CA.cer \
  "https://www.apple.com/certificateauthority/DeveloperIDG2CA.cer"
security import /tmp/DeveloperIDG2CA.cer \
  -k ~/Library/Keychains/login.keychain-db
```

### Verify the Signing Identity

```bash
security find-identity -v -p codesigning
# Should show: "Developer ID Application: University of California at San Francisco (F3YYBXAFJ8)"
```

### Install Node Dependencies

```bash
cd ui/desktop
npm install
```

### Install the Intel Rust Target (for Intel macOS builds)

```bash
rustup target add x86_64-apple-darwin
```

---

## Troubleshooting

### `@rollup/rollup-darwin-arm64` missing after Linux Docker build

The Docker `npm ci` overwrites local `node_modules` with Linux versions, breaking macOS builds.

**Fix:**
```bash
cd ui/desktop
rm -rf node_modules package-lock.json
npm install
```

### `401 Unauthorized` during notarization

The app-specific password must be generated for `wanjungu001@gmail.com` (not the UCSF email). See `APPLE_DEVELOPER_NOTES.md` for how to generate a replacement if needed.

### `cannot find -lxcb` or `cannot find -lbz2` in Linux Docker build

The cross-compilation environment needs AMD64 dev headers. This is handled automatically by the Docker command in Stage A via `dpkg --add-architecture amd64` — if you see these errors, ensure you are running the full Stage A command exactly as written above.

### Keychain access dialog during signing

Click **Always Allow** when macOS prompts for `codesign` keychain access. To suppress this prompt permanently after importing the certificate:

```bash
security set-key-partition-list -S apple-tool:,apple:,codesign: -s \
  -k "<your-login-keychain-password>" \
  ~/Library/Keychains/login.keychain-db
```

### `biorouter` binary is the wrong architecture

If you accidentally run a macOS build after a Linux Docker build, the binary in `src/bin/` may be the Linux ELF binary. Always check:

```bash
file ui/desktop/src/bin/biorouter
```

If it shows `ELF 64-bit` instead of `Mach-O`, restore it:

```bash
cp target/release/biorouter ui/desktop/src/bin/biorouter
```
