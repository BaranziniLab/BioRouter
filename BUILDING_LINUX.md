# Building Biorouter Desktop on Linux

This guide covers building the Biorouter Desktop application from source on various Linux distributions.

## Prerequisites

### System Dependencies

The reference set is the repo's own Linux build: the builder stage of the root `Dockerfile` installs `build-essential pkg-config libssl-dev libdbus-1-dev protobuf-compiler libprotobuf-dev ca-certificates`. Re-derive these lists from it rather than guessing. `libdbus-1-dev` is easy to miss — it is what the `keyring` crate's Linux Secret Service backend links against. `rpm` is needed because the RPM maker runs on a plain `npm run make` (see below).

**Debian/Ubuntu:**
```bash
sudo apt update
sudo apt install -y dpkg fakeroot rpm build-essential pkg-config \
  libssl-dev libdbus-1-dev libxcb1-dev libxcb-util-dev libbz2-dev protobuf-compiler
```

**Arch/Manjaro:**
```bash
sudo pacman -S --needed dpkg fakeroot base-devel pkgconf openssl dbus protobuf bzip2
```

**Fedora/RHEL/CentOS:**
```bash
sudo dnf install dpkg-dev fakeroot rpm-build gcc gcc-c++ make pkgconf-pkg-config \
  openssl-devel dbus-devel libxcb-devel bzip2-devel protobuf-compiler
```

**openSUSE:**
```bash
sudo zypper install dpkg fakeroot rpm-build gcc gcc-c++ make pkg-config \
  libopenssl-devel dbus-1-devel libbz2-devel protobuf-devel
```

### Development Tools

- **Rust 1.92**: Install via [rustup](https://rustup.rs/) — the channel is pinned in `rust-toolchain.toml`, so rustup selects it automatically
- **Node.js 24.x**: `ui/desktop/package.json` declares `engines: { "node": "^24.0.0" }`, and hermit pins 24.10.0. Newer majors break the Electron packaging step, so pin 24 rather than tracking latest (use [nvm](https://github.com/nvm-sh/nvm) for version management)
- **npm**: Comes with Node.js

### Runtime requirements

The bundled `llama-server` sidecar (the Llama Server local-models provider) needs OpenSSL 3 and OpenMP at runtime. The deb and rpm declare them (`libssl3` + `libgomp1`, `openssl-libs` + `libgomp`), which implies **Debian 12+ / Ubuntu 22.04+**. If you install from the zip or a flatpak, install those yourself — and on Debian 11 the app will run but local models will not.

## Build Process

### 1. Clone and Setup
```bash
git clone https://github.com/BaranziniLab/biorouter.git
cd biorouter
```

### 2. Build the Rust Backend

Build the whole workspace in release mode — the desktop bundle needs **both** the daemon (`biorouterd`) and the CLI (`biorouter`):

```bash
cargo build --release
```

### 3. Prepare the Desktop Application
```bash
cd ui/desktop
npm ci   # not `npm install` — the committed package-lock.json is the reproducible source

# Copy BOTH backend binaries to the expected location
mkdir -p src/bin
cp ../../target/release/biorouterd src/bin/
cp ../../target/release/biorouter src/bin/

# Fetch the pinned llama-server sidecar and verify all required binaries.
# This downloads llamacpp/llama-server and fails fast if biorouter or
# biorouterd is missing. Plain `npm run make` does NOT run this prep step
# (so it ships without the local-models sidecar) — run it explicitly first.
node scripts/prepare-platform-binaries.js
```

### 4. Build the Application

#### Option A: ZIP Distribution (Recommended)
Works on all Linux distributions:
```bash
npm run make -- --targets=@electron-forge/maker-zip
```

Output: `out/make/zip/linux/x64/Biorouter-linux-x64-{version}.zip`

#### Option B: DEB Package
For Debian/Ubuntu systems:
```bash
npm run make -- --targets=@electron-forge/maker-deb
```

Output: `out/make/deb/x64/biorouter_{version}_amd64.deb`

#### Option C: All Configured Makers
```bash
npm run make
```

This runs **every** maker configured in `forge.config.ts` for Linux — deb, rpm, zip and flatpak. The RPM maker is not gated by platform, so a bare `npm run make` will try to build an rpm and fail on a system without `rpm`/`rpmbuild`. Pass `--targets=` explicitly (Option A or B) if you only want one format.

### 5. Run the Application

#### From Build Directory
```bash
./out/Biorouter-linux-x64/Biorouter
```

#### Install DEB Package (if built)
```bash
sudo dpkg -i out/make/deb/x64/biorouter_*.deb
```

## Troubleshooting

### Common Issues

#### Missing System Dependencies
If you see errors about missing `dpkg` or `fakeroot`:
```bash
# Install the missing packages for your distribution (see Prerequisites above)
```

#### GLib Warnings
You may see warnings like:
```
GLib-GObject: instance has no handler with id
```
These are harmless and don't affect functionality. To suppress them, create a launcher script:

```bash
#!/bin/bash
cd /path/to/biorouter/ui/desktop/out/Biorouter-linux-x64
./Biorouter 2>&1 | grep -v "GLib-GObject" | grep -v "browser_main_loop"
```

#### Server Binary Not Found
If you see "Could not find biorouterd binary", ensure you've:
1. Built the Rust backend: `cargo build --release -p biorouter-server`
2. Copied it to the right location: `cp ../../target/release/biorouterd src/bin/`
3. Rebuilt the application: `npm run make`

### Distribution-Specific Notes

#### Arch/Manjaro
- The RPM maker is **always enabled** — it carries no `platforms` gate in `forge.config.ts` — so a bare `npm run make` (Option C) will attempt an rpm and fail on a system without `rpm`/`rpmbuild`
- Use the ZIP distribution with an explicit target for maximum compatibility: `npm run make -- --targets=@electron-forge/maker-zip`

#### Flatpak
Flatpak is configured in `forge.config.ts` (runtime 25.08, with a libbz2 shim module), but **no CI workflow and no release phase builds or tests it** — it is not a shipped artifact. To build locally:
```bash
# Install flatpak and flatpak-builder
sudo apt install flatpak flatpak-builder

# Add Flathub remote
flatpak remote-add --if-not-exists --user flathub https://dl.flathub.org/repo/flathub.flatpakrepo

# Build with Electron Forge
npm run make -- --targets=@electron-forge/maker-flatpak
```

Output: `out/make/flatpak/x86_64/*.flatpak`

#### Snap
Building as Snap packages is not currently supported but may be added in the future.

## Development Workflow

For active development:

1. **Backend changes**: Rebuild with `cargo build --release -p biorouter-server` and copy the binary
2. **Frontend changes**: Use `npm run start-gui` for hot reload during development (`npm run start` runs `just run-ui`, which does a full release `cargo build` first)
3. **Full rebuild**: Run the complete build process above

## Creating System Integration

### Desktop Entry
Create `~/.local/share/applications/biorouter.desktop`:
```ini
[Desktop Entry]
Name=Biorouter AI Agent
Comment=AI research environment for biomedical discovery
Exec=/path/to/biorouter/ui/desktop/out/Biorouter-linux-x64/Biorouter %U
Icon=/path/to/biorouter/ui/desktop/out/Biorouter-linux-x64/resources/app.asar.unpacked/src/images/icon.png
Terminal=false
Type=Application
Categories=Science;Education;Utility;
StartupNotify=true
MimeType=x-scheme-handler/biorouter
```

### System-wide Installation
To install system-wide:
```bash
sudo cp -r out/Biorouter-linux-x64 /opt/biorouter
sudo ln -s /opt/biorouter/Biorouter /usr/local/bin/biorouter-gui
```

## Contributing

When contributing changes that affect the Linux build process, please:

1. Test on multiple distributions if possible
2. Update this documentation
3. Update `ui/desktop/README.md` if needed
4. Consider CI/CD implications for automated builds
