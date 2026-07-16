# Making a Release

Biorouter releases are cut locally with [`scripts/release.sh`](scripts/release.sh), not by a
GitHub Action. The script encodes the whole pipeline — version bump → compile all four backends →
sign and notarize both macOS dmgs → package Windows and Linux → verify → publish to GitHub — so a
human or an agent can reproduce a release exactly.

Run everything under the hermit toolchain, which pins Node 24 (the macOS dmg maker's native
modules do not build on newer Node):

```bash
source bin/activate-hermit
```

## One-shot release

```bash
scripts/release.sh all 1.87.3
```

## Phase-by-phase (resumable)

Each phase is a separate subcommand, so a failed release can be resumed rather than restarted:

| Phase | What it does |
| ----- | ------------ |
| `bump <ver>` | Bump the version in the 5 release files and refresh `Cargo.lock` |
| `backends <ver>` | Compile release backends for all 4 targets (mac arm64, mac x64, windows-gnu, linux-gnu) |
| `linux-backend <ver>` | Rebuild just the Linux backend from scratch (re-runnable) |
| `mac-arm64 <ver>` | Package, sign, and **notarize** the Apple Silicon `.dmg` |
| `mac-intel <ver>` | Package, sign, and **notarize** the Intel `.dmg` |
| `windows <ver>` | Package the Windows `.zip` |
| `linux <ver>` | Package the GUI `.deb` + `.rpm` — **run this last**, it leaves `node_modules` Linux-flavored |
| `cli-linux <ver>` | Build the headless CLI-only `.deb` + `.rpm` (`biorouter` + `biorouterd`) |
| `headless-linux <ver>` | Build the browser-served headless Linux artifact |
| `mac-manifest <ver>` | Generate `latest-mac.yml` for electron-updater (also run by `publish`) |
| `verify <ver>` | Verify every artifact (architecture, notarization, dmg format) |
| `publish <ver>` | `gh release create v<ver>` with all assets and the release notes |
| `all <ver>` | Run every phase in order |

For an agent-orchestrated run where each phase is a verified subagent that stops on the first
failure, use the `release` workflow in [`.claude/workflows/release.js`](.claude/workflows/release.js).

## Version numbers

The version lives in **one** source of truth: `[workspace.package].version` in `Cargo.toml`. The CLI,
the daemon, and the core library all inherit it, so the three Rust binaries can never disagree. The
desktop GUI keeps its own copies in `ui/desktop/package.json`, `ui/desktop/package-lock.json` (2
occurrences), and `ui/desktop/openapi.json`.

**Never hand-edit a version file** — always use `scripts/release.sh bump <ver>`, which rewrites all
five in lockstep. `scripts/check-version-consistency.sh` (run by `just check-everything`) fails CI if
any of them drift.

## Release notes

Per-version notes live at `docs/release-notes/v<ver>.md`, one file per version. `publish` reads that
exact path and aborts if it is missing, so write the notes before publishing.

## After a release

The Linux packaging phase leaves a Linux-flavored `node_modules`. Restore a mac-native tree before
building again:

```bash
cd ui/desktop && rm -rf node_modules && npm install
```

## Further reading

`CLAUDE.md` carries the long-form version of this pipeline, including every hard-won invariant
(cross-compile link fixes, the one-platform-at-a-time staging rule, the Linux glibc baseline, the
exact set of release assets, and the notarization setup). `scripts/release.sh` itself documents the
same invariants inline.
