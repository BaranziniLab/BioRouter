#!/usr/bin/env bash
# Copy the macOS Biorouter Keychain secret blob into the Linux file-backed
# secret store over SSH. Secret contents are passed on stdin and not printed.
set -euo pipefail

REMOTE="${1:?usage: sync-headless-secrets-macos.sh <user@host> <ssh-key>}"
SSH_KEY="${2:?usage: sync-headless-secrets-macos.sh <user@host> <ssh-key>}"
REMOTE_SECRETS="${BIOROUTER_HEADLESS_REMOTE_SECRETS:-/home/ubuntu/.config/biorouter/secrets.yaml}"

log() { printf '\033[1;36m[headless-secrets]\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m[headless-secrets] %s\033[0m\n' "$*" >&2; exit 1; }

command -v security >/dev/null 2>&1 || die "macOS security CLI is required"
security find-generic-password -s biorouter -a secrets -w >/dev/null

log "syncing Biorouter secret blob to $REMOTE:$REMOTE_SECRETS"
security find-generic-password -s biorouter -a secrets -w | ssh \
  -i "$SSH_KEY" \
  -o BatchMode=yes \
  -o StrictHostKeyChecking=accept-new \
  "$REMOTE" "
    set -euo pipefail
    umask 077
    mkdir -p \"\$(dirname '$REMOTE_SECRETS')\"
    tmp=\$(mktemp '$REMOTE_SECRETS.tmp.XXXXXX')
    cat > \"\$tmp\"
    jq type \"\$tmp\" >/dev/null
    mv \"\$tmp\" '$REMOTE_SECRETS'
    chmod 600 '$REMOTE_SECRETS'
    grep -q '^BIOROUTER_DISABLE_KEYRING=' /etc/biorouter-headless/env \
      && sudo sed -i 's/^BIOROUTER_DISABLE_KEYRING=.*/BIOROUTER_DISABLE_KEYRING=true/' /etc/biorouter-headless/env \
      || echo 'BIOROUTER_DISABLE_KEYRING=true' | sudo tee -a /etc/biorouter-headless/env >/dev/null
    sudo chown ubuntu:ubuntu '$REMOTE_SECRETS'
    sudo systemctl restart biorouter-headless
    python3 -c 'import json,pathlib; p=pathlib.Path(\"$REMOTE_SECRETS\"); print(\"secret_keys=\"+str(len(json.loads(p.read_text())))); print(\"mode=\"+oct(p.stat().st_mode & 0o777))'
  "

log "secrets synced without printing values"
