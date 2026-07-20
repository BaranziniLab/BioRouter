# Deployment

This folder covers running BioRouter as a shared server rather than as a desktop
application: building the Linux headless artifact, deploying it to an Ubuntu host,
migrating secrets onto that host, and smoke-testing the browser UI it serves. In
this mode `biorouterd` executes the agent loop on the server and users reach it
through an ordinary browser instead of the Electron shell.

Come here when the compute belongs on a machine other than the user's laptop. If
you are installing BioRouter for yourself, start with
[installation](../getting-started/installation.md) instead. If you are cutting a
signed, notarized, multi-platform release of the desktop app, that is
[releases](../releases/local-cross-compilation.md) — the headless packaging
scripts described here are a separate path with their own artifact shape. Settings
that apply to any deployment, desktop or headless, live in
[configuration](../configuration/environment-variables.md).

## Documents in this folder

| Document | What it covers |
|---|---|
| [Headless Linux deployment](headless-linux.md) | The end-to-end procedure for building the Linux headless artifact on a Mac, verifying it carries no credentials, deploying it to an Ubuntu 22.04 or 24.04 host, migrating secrets, and smoke-testing the browser UI. Current — last verified at the 1.88.2 release. |

## Related documentation

- [Secret storage](../security/secret-storage.md) — how secrets are stored per platform, and what `BIOROUTER_DISABLE_KEYRING=true` switches to on a headless host.
- [Environment variables](../configuration/environment-variables.md) — the full set of variables the daemon and the headless service read.
- [Config file reference](../configuration/config-file-reference.md) — the server-side config directory a deployment points BioRouter at.
- [Cross-compiling locally with `cross`](../releases/local-cross-compilation.md) — building Linux binaries on macOS outside the headless packaging scripts.
