# BioRouter v1.60.0 Release Notes

**Release Date:** April 2026
**Repository:** [github.com/BaranziniLab/BioRouter](https://github.com/BaranziniLab/BioRouter)

## Downloads

This release includes native installers for all major platforms and the BioRouter CLI binary — all available in the same GitHub Release:

### GUI App

| Platform | File | Install |
| -------- | ---- | ------- |
| **macOS** (Apple Silicon / M chip) | `BioRouter-1.60.0-arm64.dmg` | Open DMG and drag `BioRouter.app` to `/Applications` |
| **macOS** (Intel) | `BioRouter-1.60.0-x64.dmg` | Open DMG and drag `BioRouter.app` to `/Applications` |
| **Windows** (x64) | `BioRouter-win32-x64-1.60.0.zip` | Unzip and run `BioRouter.exe` |
| **Linux** Ubuntu / Pop!_OS (x64) | `biorouter_1.60.0_amd64.deb` | `sudo dpkg -i biorouter_1.60.0_amd64.deb` |
| **Linux** Fedora / RHEL (x64) | `BioRouter-1.60.0-1.x86_64.rpm` | `sudo rpm -i BioRouter-1.60.0-1.x86_64.rpm` |

### CLI Binary

| Platform | File | Usage |
| -------- | ---- | ----- |
| **macOS** (Apple Silicon) | `biorouter-cli-macos-arm64` | `chmod +x biorouter-cli-macos-arm64 && ./biorouter-cli-macos-arm64` |
| **macOS** (Intel) | `biorouter-cli-macos-x64` | `chmod +x biorouter-cli-macos-x64 && ./biorouter-cli-macos-x64` |
| **Linux** (x64) | `biorouter-cli-linux-x64` | `chmod +x biorouter-cli-linux-x64 && ./biorouter-cli-linux-x64` |
| **Windows** (x64) | `biorouter-cli-windows-x64.exe` | Run directly from Command Prompt or PowerShell |

## What's New

### Expanded Model Support — All Providers Updated

Comprehensive model list update across every provider, verified against live APIs.

#### Amazon Bedrock (UCSF MuleSoft Proxy)

- Default model updated to `us.anthropic.claude-sonnet-4-6`
- Confirmed available: Claude Sonnet 4.6, Claude Sonnet 4.0, Claude Opus 4.1, Claude Opus 4.5
- Removed models the UCSF proxy IAM policy does not permit — no more silent auth failures

#### Azure OpenAI (UCSF Unified API)

- Default model updated to `gpt-5.2-2025-12-11`
- Confirmed available: GPT-5.2, GPT-4.1, GPT-4.1-mini, GPT-4o, o1, o3-mini, o4-mini
- API version bumped to `2025-01-01-preview` — unlocks o1/o3-mini/o4-mini reasoning models

#### Anthropic (Direct API)

- Default updated to `claude-sonnet-4-6`
- Added Claude 4.6 family: `claude-opus-4-6`, `claude-sonnet-4-6`

#### OpenAI (Direct API)

- Default updated to `gpt-5.2`
- Added full GPT-5 series: `gpt-5`, `gpt-5.1`, `gpt-5.1-codex`, `gpt-5.2`, `gpt-5.4`
- Added `o3-mini`; updated context window sizes throughout

#### Google (Direct API)

- Added Gemini 3 series: `gemini-3-flash-preview`, `gemini-3-pro-preview`, `gemini-3-pro-image-preview`
- Default remains `gemini-2.5-pro`

### GPT-5.4 Support via OpenAI Responses API

GPT-5.4 requires OpenAI's `/v1/responses` endpoint rather than `/v1/chat/completions`. BioRouter now automatically routes requests to the correct endpoint based on the model — users simply select `gpt-5.4` and everything works transparently. Full tool use and streaming are supported.

### Amazon Bedrock Endpoint Normalization

Users setting `AWS_ENDPOINT_URL_BEDROCK` (the intuitive short form) previously got a silent authentication failure because the AWS SDK for Rust expects `AWS_ENDPOINT_URL_BEDROCK_RUNTIME`. BioRouter now auto-promotes the short form to the correct key before the SDK initializes — both names work transparently.

### Session History Fix (upgrade from pre-v1.50.0)

Users upgrading from older versions saw "Error Loading Sessions" due to a column rename (`recipe_json` → `workflow_json`) that lacked a database migration. Migration v7 now runs automatically on startup and safely renames the columns on existing databases, restoring full chat history for all users.

### BioRouter CLI Included in Releases

The `biorouter` CLI binary is now published alongside the GUI installer for all platforms. The CLI provides the same agent capabilities as the desktop app and can be used in terminal workflows, scripts, and headless environments.

## Bug Fixes

- Fixed `AWS_ENDPOINT_URL_BEDROCK` not being recognized by the AWS SDK; the correct `AWS_ENDPOINT_URL_BEDROCK_RUNTIME` is now set automatically
- Fixed "Error Loading Sessions" on databases created before the v1.50.0 workflow rename
- Bumped Azure default API version from `2024-10-21` to `2025-01-01-preview` to support o-series reasoning models
- Removed model IDs unavailable on UCSF proxies, eliminating confusing error messages on model selection

## Upgrading

No manual migration needed. Simply replace your existing installation with the new package for your platform.

- On macOS, replace `BioRouter.app` in `/Applications` with the new one from the DMG
- On Linux, install the new `.deb` or `.rpm` over the existing installation (`dpkg -i` and `rpm -U` handle upgrades automatically)
- On Windows, unzip and replace the existing installation folder

## Known Limitations

- **gpt-5.4 tool use**: While gpt-5.4 works via the Responses API, complex multi-tool conversations may behave differently from chat-completions models
- **Google free tier**: `gemini-2.5-pro` and `gemini-2.0-flash` require a paid Google AI API plan; `gemini-2.5-flash` works on the free tier
- **Auto-update**: Auto-update is not yet enabled; check GitHub Releases for new versions

*UCSF BioRouter is developed by the [Baranzini Lab](https://baranzinilab.ucsf.edu/) at the University of California, San Francisco.*
*Licensed under the [Apache 2.0 License](https://opensource.org/licenses/Apache-2.0).*
