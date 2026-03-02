# BioRouter v1.20.0 Release Notes

**Release Date:** February 2026
**Repository:** [github.com/BaranziniLab/BioRouter](https://github.com/BaranziniLab/BioRouter)


## Downloads

This release includes native installers for all major platforms — all available in the same GitHub Release:

| Platform | File | Install |
|----------|------|---------|
| **macOS** (Apple Silicon / M chip) | `BioRouter-1.20.0-arm64.dmg` | Open DMG and drag `BioRouter.app` to `/Applications` |
| **macOS** (Intel) | `BioRouter-1.20.0-x64.dmg` | Open DMG and drag `BioRouter.app` to `/Applications` |
| **Windows** (x64) | `BioRouter-win32-x64-1.20.0.zip` | Unzip and run `BioRouter.exe` |
| **Linux** Ubuntu / Pop!_OS (x64) | `biorouter_1.20.0_amd64.deb` | `sudo dpkg -i biorouter_1.20.0_amd64.deb` |
| **Linux** Fedora / RHEL (x64) | `BioRouter-1.20.0-1.x86_64.rpm` | `sudo rpm -i BioRouter-1.20.0-1.x86_64.rpm` |


## What's New

### New Default Models

Updated default model selections across providers to reflect the latest available options (Claude Sonnet 4, GPT-4.1, and others), ensuring users get the best out-of-the-box experience without manual configuration.

### Refined System Prompts

Improved agent system prompts for the desktop, sub-agent, and recipe instruction contexts — better task decomposition, more consistent behavior across providers, and clearer identity framing for the BioRouter agent.

### Updated Logo & Branding

New UCSF BioRouter logo applied throughout the app — desktop icon, documentation site, and in-app UI all use the updated visual identity.

### Simplified Provider Settings

Cleaned up the provider configuration panel — removed unused provider entries and streamlined the grid layout so only supported, actively maintained providers are shown.

### Streamlined Chat Settings

Removed the BioRouter hints modal and associated chat configuration options that added friction without value. The chat interface is now cleaner and more focused.

### Simplified Landing Page

The provider guard/onboarding screen has been significantly simplified — reduced from ~200 lines of configuration UI to a focused entry point that gets users to the chat faster.

### Removed Dictation Feature

Removed the experimental voice dictation input from the chat interface to reduce complexity and focus on core text-based workflows.

### Simplified App Settings

Cleaned up the app settings panel — streamlined update configuration and removed settings that were not yet functional.

### Documentation Suite

Added a full documentation set under `documentation/`:

- **Architecture** — how BioRouter components fit together
- **Installation & Setup** — step-by-step guide for all platforms
- **Providers & Models** — supported LLM providers and configuration
- **Extensions, Skills & MCP** — how to extend BioRouter with tools
- **Recipes** — authoring and running automated workflows
- **Schedulers** — time-based and event-driven task automation
- **Data Privacy** — what data is collected and how it is handled

### Multi-Platform Build Infrastructure

Added complete cross-compilation pipelines for Windows and Linux from a macOS development machine:

- Windows x64 via Docker + `mingw-w64` cross-compiler
- Linux x64 via Docker + `x86_64-unknown-linux-gnu` Rust target
- Electron Forge packaging for `.deb` and `.rpm` inside a `linux/amd64` Docker container

### macOS — Signed & Notarized

Both macOS builds (Apple Silicon and Intel) are signed with a Developer ID certificate and notarized by Apple. The app opens without Gatekeeper warnings on supported systems.


## Bug Fixes & Internal Changes

- Updated `rmcp` dependency to the latest version for improved MCP protocol compatibility
- Fixed Azure provider default model configuration
- Removed unused `jbang`, `npx`, `uvx`, and `node` helper scripts from bundled binaries (tools are now sourced from the system or user environment)
- Improved stability of the backend Rust service on startup
- Fixed drag-and-drop support for folders onto the dock icon
- Cleaned up GitHub Actions CI workflows


## Known Limitations

- **Flatpak:** A Linux Flatpak package is not included in this release — it requires additional build tooling (`flatpak-builder`) that will be added to the CI pipeline in a future release.
- **Auto-update:** Auto-update is not yet enabled; check GitHub Releases for new versions.

## Upgrading

There is no automatic migration needed. Simply replace your existing installation with the new package for your platform.

- On macOS, replace the existing `BioRouter.app` in `/Applications` with the new one from the DMG.
- On Linux, install the new `.deb` or `.rpm` over the existing installation (`dpkg -i` and `rpm -U` handle upgrades automatically).


*UCSF BioRouter is developed by the [Baranzini Lab](https://baranzinilab.ucsf.edu/) at the University of California, San Francisco.*
*Licensed under the [Apache 2.0 License](https://opensource.org/licenses/Apache-2.0).*
