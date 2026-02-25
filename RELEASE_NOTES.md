# BioRouter v1.20.0 Release Notes

## Downloads

Native installers are available for all major platforms in this release:

| Platform | Package | Install |
|----------|---------|---------|
| **macOS** (Apple Silicon) | `.zip` | Unzip and drag `BioRouter.app` to `/Applications` |
| **Windows** (x64) | `.zip` | Unzip and run `BioRouter.exe` |
| **Linux** Ubuntu / Pop!_OS (x64) | `.deb` | `sudo dpkg -i biorouter_*.deb` |
| **Linux** Fedora / RHEL (x64) | `.rpm` | `sudo rpm -i BioRouter-*.rpm` |

---

## What's New

### Multi-Platform Distribution
BioRouter is now distributed for macOS, Windows, and Linux in the same release. All platform builds are available under the GitHub Releases assets.

### macOS — Signed & Notarized
The macOS build is now signed with a Developer ID certificate and notarized by Apple. This means the app will open on macOS without Gatekeeper warnings on supported systems.

### Linux Support
BioRouter is now available as a native `.deb` (Ubuntu / Pop!_OS) and `.rpm` (Fedora / RHEL) package for Linux x64.

### Windows Support
BioRouter is available as a Windows x64 `.zip` containing the native executable.

---

## macOS Installation Notes

1. Download `BioRouter.zip` from the release assets.
2. Unzip and move `BioRouter.app` to your `/Applications` folder.
3. On first launch, run the following command in Terminal to clear the quarantine flag:

```bash
xattr -cr /Applications/BioRouter.app
```

4. Double-click `BioRouter.app` to open it.

### If the app won't open

- Make sure you've run the `xattr -cr` command above.
- Try the **right-click + Option + Open** method: hold Option, right-click the app, and select Open.
- Check **System Preferences → Security & Privacy** — if macOS blocked the app, there will be an "Open Anyway" button in the General tab.

---

## Bug Fixes

- Improved stability of the backend Rust service on startup.
- Fixed drag-and-drop support for folders onto the dock icon.
- Various UI polish and performance improvements.

---

## Known Limitations

- macOS Intel (x64) builds are not included in this release. Apple Silicon (arm64) only.
- Flatpak distribution for Linux is not yet available.
- Auto-update is not yet enabled; check GitHub Releases for new versions.

---

## Upgrade Notes

If upgrading from a previous version:
- Quit BioRouter before installing the new version.
- On macOS, replace the existing `BioRouter.app` in `/Applications` with the new one.
- On Linux, install the new `.deb` or `.rpm` over the existing installation (`dpkg -i` and `rpm -U` handle upgrades automatically).

---

*BioRouter is developed at the Baranzini Lab, University of California, San Francisco.*
