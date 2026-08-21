# Biorouter Desktop App

Native desktop app for Biorouter built with [Electron](https://www.electronjs.org/) and [ReactJS](https://react.dev/).

# Building and running
Biorouter uses [Hermit](https://github.com/cashapp/hermit) to manage dependencies, so you will need to have it installed and activated.

```
git clone git@github.com:BaranziniLab/biorouter.git
cd biorouter
source ./bin/activate-hermit
cd ui/desktop
npm install
npm run start
```

## Platform-specific build requirements

### Linux
For building on Linux distributions, you'll need additional system dependencies:

**Debian/Ubuntu:**
```bash
sudo apt install dpkg fakeroot
```

**Arch/Manjaro:**
```bash
sudo pacman -S dpkg fakeroot
```

**Fedora/RHEL:**
```bash
sudo dnf install dpkg-dev fakeroot
```

# Building notes

This is an electron forge app, using vite and react.js. `biorouterd` runs as multi process binaries on each window/tab similar to chrome.

## Building for different platforms

### macOS
`npm run bundle:default` will give you a Biorouter.app/zip which is signed/notarized but only if you setup the env vars as per `forge.config.ts` (you can empty out the section on osxSign if you don't want to sign it) - this will have all defaults.

`npm run bundle:preconfigured` will make a Biorouter.app/zip signed and notarized, but use the following:

```python
            f"        process.env.BIOROUTER_PROVIDER__TYPE = '{os.getenv("BIOROUTER_BUNDLE_TYPE")}';",
            f"        process.env.BIOROUTER_PROVIDER__HOST = '{os.getenv("BIOROUTER_BUNDLE_HOST")}';",
            f"        process.env.BIOROUTER_PROVIDER__MODEL = '{os.getenv("BIOROUTER_BUNDLE_MODEL")}';"
```

This allows you to set for example BIOROUTER_PROVIDER__TYPE to be "databricks" by default if you want (so when people start biorouter.app - they will get that out of the box). Only use providers that support OAuth, otherwise use the default Biorouter.

### Linux
For Linux builds, first ensure you have the required system dependencies installed (see above), then:

1. Build the Rust backend:
```bash
cd ../..  # Go to project root
cargo build --release -p biorouter-server
```

2. Copy the server binary to the expected location:
```bash
mkdir -p src/bin
cp ../../target/release/biorouterd src/bin/
```

3. Build the application:
```bash
# For ZIP distribution (works on all Linux distributions)
npm run make -- --targets=@electron-forge/maker-zip

# For DEB package (Debian/Ubuntu)
npm run make -- --targets=@electron-forge/maker-deb

# For Flatpak (requires flatpak and flatpak-builder)
npm run make -- --targets=@electron-forge/maker-flatpak
```

The built application will be available in:
- ZIP: `out/make/zip/linux/x64/biorouter-linux-x64-{version}.zip`
- DEB: `out/make/deb/x64/biorouter_{version}_amd64.deb`
- Flatpak: `out/make/flatpak/x86_64/*.flatpak`
- Executable: `out/biorouter-linux-x64/biorouter`

### Windows

The release artifact is a Windows x64 ZIP. The supported release path can run
from a macOS development machine: Docker cross-compiles the GNU Rust binaries,
then Electron Forge packages those staged binaries on the host.

#### From macOS with Docker (release path)

Use the system Docker installation rather than the Hermit Docker shim. From the
repository root, build the backends and then package Windows for the current
version:

```bash
source ./bin/activate-hermit
scripts/release.sh backends 1.89.2
scripts/release.sh windows 1.89.2
```

For a Windows-only backend rebuild, `just release-windows` uses the same Docker
recipe as CI. Before packaging, make sure `biorouter.exe`, `biorouterd.exe`, and
the MinGW runtime DLLs are present in
`target/x86_64-pc-windows-gnu/release/`. The release script stages them into
`ui/desktop/src/bin/` automatically.

#### Directly on Windows (local validation)

Install Visual Studio Build Tools with the C++ workload, CMake, NASM, Rust
1.92, and Node 24. `aws-lc-sys` requires NASM for release builds; disabling
assembly is not supported in release mode. PowerShell can use every logical CPU
for the Rust build as follows:

```powershell
$jobs = (Get-CimInstance Win32_ComputerSystem).NumberOfLogicalProcessors
cargo build --release --bin biorouter --bin biorouterd --jobs $jobs
New-Item -ItemType Directory -Force ui\desktop\src\bin | Out-Null
Copy-Item target\release\biorouter.exe,target\release\biorouterd.exe ui\desktop\src\bin\
Set-Location ui\desktop
npm ci
npm run bundle:windows
```

The ZIP is written to
`out/make/zip/win32/x64/Biorouter-win32-x64-<version>.zip`.

#### Windows release checks and takeaways

- Keep `bundle:windows` implemented as a Node script. Inline POSIX environment
  assignments such as `ELECTRON_PLATFORM=win32 command` do not run in Windows
  Command Prompt or PowerShell.
- Use Node libraries for archive handling. The Windows llama.cpp download is a
  ZIP and must not depend on a host `unzip` executable.
- `ELECTRON_PLATFORM=win32` and `ELECTRON_ARCH=x64` must apply to MinGit
  download, binary preparation, and Electron Forge packaging. Platform bundles
  overwrite `src/bin`, so build and stage one platform at a time.
- Electron Forge's Windows ZIP contains `Biorouter.exe` and `resources/` at the
  archive root; there is no extra `Biorouter-win32-x64/` directory after
  extraction.
- Do not consider the release complete until the ZIP itself has been extracted
  on Windows, both embedded binaries report the expected version,
  `biorouter term --help` succeeds, and `Biorouter.exe` remains running through
  startup. The `Release artifact smoke` GitHub workflow performs these checks
  against a draft release.


# Running with biorouterd server from source

Set `VITE_START_EMBEDDED_SERVER=yes` to no in `.env`.
Run `cargo run -p biorouter-server` from parent dir.
`npm run start` will then run against this.
You can try server directly with `./test.sh`
