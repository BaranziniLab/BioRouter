#!/usr/bin/env bash
# Deploy dist/headless-linux-x64 to an Ubuntu host and run the headless
# systemd provisioning script.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARTIFACT="$ROOT/dist/headless-linux-x64"
SETUP_SCRIPT="$ROOT/scripts/setup-headless-ubuntu.sh"
REMOTE="${1:?usage: deploy-headless-linux.sh <user@host> <ssh-key>}"
SSH_KEY="${2:?usage: deploy-headless-linux.sh <user@host> <ssh-key>}"
REMOTE_STAGE="${BIOROUTER_HEADLESS_REMOTE_STAGE:-/home/ubuntu/biorouter-headless}"
REMOTE_INSTALL="${BIOROUTER_HEADLESS_REMOTE_INSTALL:-/opt/biorouter-headless}"

log() { printf '\033[1;36m[headless-deploy]\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m[headless-deploy] %s\033[0m\n' "$*" >&2; exit 1; }

[ -d "$ARTIFACT/bin" ] || die "missing $ARTIFACT/bin; run scripts/build-headless-linux.sh first"
[ -d "$ARTIFACT/web" ] || die "missing $ARTIFACT/web; run scripts/build-headless-linux.sh first"
[ -f "$SETUP_SCRIPT" ] || die "missing $SETUP_SCRIPT"

log "syncing artifact to $REMOTE:$REMOTE_STAGE"
rsync -az --delete \
  -e "ssh -i $SSH_KEY -o BatchMode=yes -o StrictHostKeyChecking=accept-new" \
  "$ARTIFACT/" "$REMOTE:$REMOTE_STAGE/"
scp -i "$SSH_KEY" -o BatchMode=yes -o StrictHostKeyChecking=accept-new \
  "$SETUP_SCRIPT" "$REMOTE:$REMOTE_STAGE/setup-headless-ubuntu.sh"

log "provisioning Ubuntu headless service"
ssh -i "$SSH_KEY" -o BatchMode=yes "$REMOTE" "
  set -euo pipefail
  BIOROUTER_HEADLESS_STAGE='$REMOTE_STAGE' \
  BIOROUTER_HEADLESS_INSTALL='$REMOTE_INSTALL' \
  bash '$REMOTE_STAGE/setup-headless-ubuntu.sh'
  sudo systemctl is-active biorouter-headless
  '$REMOTE_INSTALL/bin/biorouter' --version
  '$REMOTE_INSTALL/bin/biorouterd' --version
  '$REMOTE_INSTALL/bin/biorouter-headless' --version
  /usr/local/bin/biorouter-headless-url
"

log "deployed $ARTIFACT"
