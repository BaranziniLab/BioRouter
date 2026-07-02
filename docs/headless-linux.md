# Headless Linux deployment

This flow builds BioRouter on the Mac, deploys the Linux artifact to an
Ubuntu 22.04/24.04 host, and serves the browser UI from that host while
`biorouterd` does the compute locally on the server.

## Build locally

```bash
scripts/build-headless-linux.sh
```

The artifact is written to `dist/headless-linux-x64/`:

- `bin/biorouter`
- `bin/biorouterd`
- `bin/biorouter-headless`
- `web/`
- `manifest.txt`

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

This reads the BioRouter macOS Keychain item and sends it over SSH into the
Ubuntu file-backed secret store at `~/.config/biorouter/secrets.yaml`. Secret
values are not printed. The service uses `BIOROUTER_DISABLE_KEYRING=true` on
headless Linux.

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
