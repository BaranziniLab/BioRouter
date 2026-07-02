# Headless Linux deployment

This flow builds BioRouter on the Mac, deploys the Linux artifact to an
Ubuntu 22.04/24.04 host, and serves the browser UI from that host while
`biorouterd` does the compute locally on the server.

## Agent quickstart

For a future coding agent that is asked to "build the app" or "compile the
headless Debian binary", run this from the repository root:

```bash
source bin/activate-hermit
scripts/package-headless-linux.sh
```

That command builds the Linux x64 binaries and browser UI, verifies the artifact
does not contain local profiles or credential material, and writes:

- `dist/headless-linux-x64/`
- `dist/biorouter-headless-linux-x64.tar.gz`

Use the tarball as the portable Debian/Ubuntu release artifact. Use the
directory for deployment and inspection.

## Build locally

```bash
source bin/activate-hermit
scripts/build-headless-linux.sh
```

The artifact is written to `dist/headless-linux-x64/`:

- `bin/biorouter`
- `bin/biorouterd`
- `bin/biorouter-headless`
- `web/`
- `manifest.txt`

The build intentionally packages app deliverables only. It copies the three
Linux binaries from the Docker build output and the static browser bundle from
Vite. It must not copy any of the following into `dist/headless-linux-x64/`:

- `~/.config/biorouter/`
- `~/.aws/`
- `~/.ssh/`
- `~/Library/Application Support/`
- `secrets.yaml`, `config.yaml`, or `sessions.db`
- local `.env` files, OpenRouter keys, AWS keys, SSH keys, or downloaded access
  key CSV files

Keep credential setup as a runtime concern. A deployment can point BioRouter at
a server-side config directory, but release artifacts must stay profile-free.

## Verify and package

Run the artifact verifier before sharing or deploying a release build:

```bash
scripts/verify-headless-artifact.sh --tar
```

The verifier checks:

- the expected artifact shape: `bin/`, `web/`, and `manifest.txt`
- Linux x86_64 ELF binaries for `biorouter`, `biorouterd`, and
  `biorouter-headless`
- no packaged profile or credential-store file names
- no obvious local paths or key-like material in artifact contents

It intentionally prints only file names for failures, not matched secret text.
If it fails, inspect the named files locally, remove the leak, rebuild, and rerun
the verifier.

## Deploy to Ubuntu

```bash
scripts/deploy-headless-linux.sh ubuntu@HOST /path/to/key.pem
```

The deploy script syncs `dist/headless-linux-x64/`, runs
`scripts/setup-headless-ubuntu.sh` on the host, and prints the browser URL.

The Ubuntu setup script:

- verifies Ubuntu 22.04 or 24.04
- installs Xvfb, Linux automation helpers, `jq`, `rsync`, and `uv`
- installs the artifact under `/opt/biorouter-headless`
- configures `biorouter-headless.service`, which serves the UI, proxies `/api/*`,
  exposes `/headless/*`, and supervises `biorouterd`
- configures `biorouter-xvfb.service`
- installs `/usr/local/bin/biorouter-headless-url`
- rewrites copied macOS extension paths to Ubuntu paths
- rebuilds copied extension virtualenvs natively on Ubuntu

## Sync secrets from macOS

```bash
scripts/sync-headless-secrets-macos.sh ubuntu@HOST /path/to/key.pem
```

This is a separate runtime migration helper, not a packaging step. It reads the
BioRouter macOS Keychain item and sends it over SSH into the Ubuntu file-backed
secret store at `~/.config/biorouter/secrets.yaml`. Secret values are not
printed. The service uses `BIOROUTER_DISABLE_KEYRING=true` on headless Linux.

Do not run this script when creating a release artifact for users. A user who
starts headless BioRouter without credentials should be prompted by the app to
configure providers in the browser UI.

## Get the browser URL

On the Ubuntu host:

```bash
biorouter-headless-url
```

The URL is clean; `biorouter-headless` injects the local `X-Secret-Key` header
when proxying browser requests to `biorouterd`, so the secret is not carried in
the browser URL.

## Smoke checks

```bash
scripts/test-headless-linux.sh ubuntu@HOST /path/to/key.pem
scripts/test-headless-linux.sh ubuntu@HOST /path/to/key.pem --live
```

The default smoke test checks OS version, systemd services, emitted URL, the
public headless proxy, `/headless/health`, API status, provider/session/app
visibility, provider model catalogs, copied extension path portability, the
remote folder chooser bridge, non-empty Skills UI, and the browser UI. The
browser check loads the emitted URL in Chromium or locally installed Chrome,
verifies that the app is not blank, confirms the browser preload bridges are
present, opens Settings, checks model/provider controls, opens Skills, confirms
built-in skills are visible, and fails on relevant browser console warnings or
errors. The `--live` mode additionally sends short completion requests through
low-cost providers.

Provider-specific failures can still be external to BioRouter. For example,
an institutional provider may reject a new EC2 public IP until it is allowlisted,
and a provider account can reject requests for billing reasons.

## UI synchronization notes

The headless app uses the same `ui/desktop` renderer as the desktop shell, built
with `vite.renderer.config.mts`. UI changes for browser readability should be
made in shared renderer components, then rebuilt with the headless scripts above.

For wide-browser layouts, current shared surfaces use the `ReadableContent`
wrapper and `.biorouter-readable-content` class to constrain reading width. When
checking for drift between desktop and headless, inspect the shared source first
and then validate the deployed browser DOM rather than relying on an unauthenticated
local Vite renderer, which may fall into onboarding without the daemon/config
state needed for data-backed routes.
