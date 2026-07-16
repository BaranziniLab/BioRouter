# Building Biorouter Desktop on Linux

This guide covers building the Biorouter Desktop application from source on various Linux distributions.

## Prerequisites

### System Dependencies

**Debian/Ubuntu:**
```bash
sudo apt update
sudo apt install -y dpkg fakeroot build-essential libxcb1-dev libxcb-util-dev protobuf-compiler
```

**Arch/Manjaro:**
```bash
sudo pacman -S --needed dpkg fakeroot base-devel
```

**Fedora/RHEL/CentOS:**
```bash
sudo dnf install dpkg-dev fakeroot gcc gcc-c++ make libxcb-devel
```

**openSUSE:**
```bash
sudo zypper install dpkg fakeroot gcc gcc-c++ make
```

### Development Tools

- **Rust**: Install via [rustup](https://rustup.rs/)
- **Node.js**: Version 24 or later (use [nvm](https://github.com/nvm-sh/nvm) for version management)
- **npm**: Comes with Node.js

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
npm install

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

#### Option C: Both Formats
```bash
npm run make
```

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
- The RPM maker is disabled by default as it's not compatible with Arch-based systems
- Use the ZIP distribution method for maximum compatibility

#### Flatpak
Flatpak builds are supported via CI. To build locally:
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
