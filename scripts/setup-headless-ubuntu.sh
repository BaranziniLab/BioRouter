#!/usr/bin/env bash
# Provision an Ubuntu 22.04/24.04 host for a headless BioRouter deployment.
# Run on the Ubuntu host after dist/headless-linux-x64 has been synced.
set -euo pipefail

STAGE="${BIOROUTER_HEADLESS_STAGE:-/home/ubuntu/biorouter-headless}"
INSTALL="${BIOROUTER_HEADLESS_INSTALL:-/opt/biorouter-headless}"
ENV_FILE="${BIOROUTER_HEADLESS_ENV_FILE:-/etc/biorouter-headless/env}"
PORT="${BIOROUTER_HEADLESS_PUBLIC_PORT:-8080}"
API_HOST="${BIOROUTER_HEADLESS_API_HOST:-127.0.0.1}"
API_PORT="${BIOROUTER_HEADLESS_API_PORT:-3000}"
DISPLAY_ID="${BIOROUTER_HEADLESS_DISPLAY:-:99}"
SERVICE_USER="${BIOROUTER_HEADLESS_USER:-ubuntu}"
SERVICE_HOME="$(eval echo "~$SERVICE_USER")"

log() { printf '\033[1;36m[headless-setup]\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m[headless-setup] %s\033[0m\n' "$*" >&2; exit 1; }

require_ubuntu() {
  [ -r /etc/os-release ] || die "cannot read /etc/os-release"
  # shellcheck disable=SC1091
  . /etc/os-release
  [ "${ID:-}" = "ubuntu" ] || die "expected Ubuntu, got ${PRETTY_NAME:-unknown}"
  case "${VERSION_ID:-}" in
    22.04|24.04) ;;
    *) die "expected Ubuntu 22.04 or 24.04, got ${VERSION_ID:-unknown}" ;;
  esac
  log "host OS: ${PRETTY_NAME:-Ubuntu}"
}

install_packages() {
  export DEBIAN_FRONTEND=noninteractive
  sudo apt-get update -qq
  sudo apt-get install -y --no-install-recommends \
    ca-certificates curl jq rsync openssl python3 \
    xvfb xauth xdotool wmctrl xclip x11-utils >/tmp/biorouter-headless-apt.log
}

install_uv() {
  if [ ! -x "$SERVICE_HOME/.local/bin/uv" ]; then
    curl -LsSf https://astral.sh/uv/install.sh | sh
  fi
  local uv_bin
  uv_bin="$SERVICE_HOME/.local/bin/uv"
  [ -x "$uv_bin" ] || die "uv install did not produce $uv_bin"
  sudo rm -f /usr/local/bin/uv
  sudo ln -sf "$uv_bin" /usr/local/bin/uv
  /usr/local/bin/uv --version >/dev/null
}

install_artifact() {
  [ -x "$STAGE/bin/biorouterd" ] || die "missing $STAGE/bin/biorouterd"
  [ -x "$STAGE/bin/biorouter" ] || die "missing $STAGE/bin/biorouter"
  [ -x "$STAGE/bin/biorouter-headless" ] || die "missing $STAGE/bin/biorouter-headless"
  [ -f "$STAGE/web/index.html" ] || die "missing $STAGE/web/index.html"
  sudo mkdir -p "$INSTALL"
  sudo rsync -a --delete "$STAGE/" "$INSTALL/"
  sudo chown -R "$SERVICE_USER:$SERVICE_USER" "$INSTALL"
}

ensure_env() {
  sudo mkdir -p "$(dirname "$ENV_FILE")"
  sudo touch "$ENV_FILE"
  sudo chmod 600 "$ENV_FILE"

  if ! sudo grep -q '^BIOROUTER_SERVER__SECRET_KEY=' "$ENV_FILE"; then
    printf 'BIOROUTER_SERVER__SECRET_KEY=%s\n' "$(openssl rand -hex 32)" | sudo tee -a "$ENV_FILE" >/dev/null
  fi

  upsert_env BIOROUTER_HOST "$API_HOST"
  upsert_env BIOROUTER_PORT "$API_PORT"
  upsert_env BIOROUTER_DISABLE_KEYRING true
  upsert_env BIOROUTER_HEADLESS_PUBLIC_PORT "$PORT"
  upsert_env BIOROUTER_HEADLESS_WEB_DIR "$INSTALL/web"
  upsert_env BIOROUTER_HEADLESS_BIOROUTERD "$INSTALL/bin/biorouterd"
  upsert_env DISPLAY "$DISPLAY_ID"
}

upsert_env() {
  local key="$1"
  local value="$2"
  if sudo grep -q "^${key}=" "$ENV_FILE"; then
    sudo sed -i "s#^${key}=.*#${key}=${value}#" "$ENV_FILE"
  else
    printf '%s=%s\n' "$key" "$value" | sudo tee -a "$ENV_FILE" >/dev/null
  fi
}

server_secret() {
  sudo sed -n 's/^BIOROUTER_SERVER__SECRET_KEY=//p' "$ENV_FILE" | tail -n1
}

configure_systemd() {
  local display_num="${DISPLAY_ID#:}"
  sudo tee /etc/systemd/system/biorouter-xvfb.service >/dev/null <<EOF
[Unit]
Description=Virtual X display for BioRouter headless automation
After=network-online.target

[Service]
Type=simple
User=$SERVICE_USER
ExecStart=/usr/bin/Xvfb :$display_num -screen 0 1920x1080x24 -nolisten tcp
Restart=on-failure
RestartSec=3

[Install]
WantedBy=multi-user.target
EOF

  sudo tee /etc/systemd/system/biorouter-headless.service >/dev/null <<EOF
[Unit]
Description=BioRouter headless daemon
After=network-online.target biorouter-xvfb.service
Wants=network-online.target biorouter-xvfb.service

[Service]
Type=simple
User=$SERVICE_USER
WorkingDirectory=$SERVICE_HOME
EnvironmentFile=$ENV_FILE
ExecStart=$INSTALL/bin/biorouter-headless serve --host 0.0.0.0 --port $PORT --web-dir $INSTALL/web --biorouterd $INSTALL/bin/biorouterd --api-host $API_HOST --api-port $API_PORT
Restart=on-failure
RestartSec=3

[Install]
WantedBy=multi-user.target
EOF
}

metadata_public_ip() {
  local token ip
  token="$(curl -fsS --max-time 2 -X PUT \
    -H 'X-aws-ec2-metadata-token-ttl-seconds: 60' \
    http://169.254.169.254/latest/api/token 2>/dev/null || true)"
  if [ -n "$token" ]; then
    ip="$(curl -fsS --max-time 2 \
      -H "X-aws-ec2-metadata-token: $token" \
      http://169.254.169.254/latest/meta-data/public-ipv4 2>/dev/null || true)"
  else
    ip="$(curl -fsS --max-time 2 http://169.254.169.254/latest/meta-data/public-ipv4 2>/dev/null || true)"
  fi
  printf '%s' "$ip"
}

install_url_command() {
  sudo tee /usr/local/bin/biorouter-headless-url >/dev/null <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
ENV_FILE="${BIOROUTER_HEADLESS_ENV_FILE:-/etc/biorouter-headless/env}"
PORT="${BIOROUTER_HEADLESS_PUBLIC_PORT:-8080}"
if [ -r "$ENV_FILE" ]; then
  # shellcheck disable=SC1090
  . "$ENV_FILE"
fi
if [ -n "${BIOROUTER_HEADLESS_PUBLIC_URL:-}" ]; then
  printf '%s\n' "$BIOROUTER_HEADLESS_PUBLIC_URL"
  exit 0
fi
TOKEN="$(curl -fsS --max-time 2 -X PUT -H 'X-aws-ec2-metadata-token-ttl-seconds: 60' http://169.254.169.254/latest/api/token 2>/dev/null || true)"
if [ -n "$TOKEN" ]; then
  HOST="$(curl -fsS --max-time 2 -H "X-aws-ec2-metadata-token: $TOKEN" http://169.254.169.254/latest/meta-data/public-ipv4 2>/dev/null || true)"
else
  HOST="$(curl -fsS --max-time 2 http://169.254.169.254/latest/meta-data/public-ipv4 2>/dev/null || true)"
fi
if [ -z "${HOST:-}" ]; then
  HOST="$(hostname -I | awk '{print $1}')"
fi
printf 'http://%s:%s/\n' "$HOST" "$PORT"
EOF
  sudo chmod +x /usr/local/bin/biorouter-headless-url
}

repair_extension_paths() {
  local config="$SERVICE_HOME/.config/biorouter/config.yaml"
  [ -f "$config" ] || return 0
  cp "$config" "$config.bak.$(date +%Y%m%d%H%M%S)"
  perl -0pi -e "s#/Users/[^/]+/\\.config/biorouter/extensions#$SERVICE_HOME/.config/biorouter/extensions#g" "$config"

  local ext_root="$SERVICE_HOME/.config/biorouter/extensions"
  [ -d "$ext_root" ] || return 0
  while IFS= read -r pyproject; do
    local dir
    dir="$(dirname "$pyproject")"
    rm -rf "$dir/.venv"
    log "rebuilding extension venv: $(basename "$dir")"
    /usr/local/bin/uv sync --directory "$dir"
  done < <(find "$ext_root" -maxdepth 2 -name pyproject.toml -print | sort)
}

repair_knowledge_paths() {
  local registry="$SERVICE_HOME/.config/biorouter/knowledge/registry.yaml"
  [ -f "$registry" ] || return 0
  cp "$registry" "$registry.bak.$(date +%Y%m%d%H%M%S)"
  perl -0pi -e "s#/Users/[^/]+/\\.config/biorouter/knowledge#$SERVICE_HOME/.config/biorouter/knowledge#g" "$registry"
}

start_services() {
  sudo systemctl daemon-reload
  if systemctl list-unit-files nginx.service >/dev/null 2>&1; then
    sudo systemctl disable --now nginx.service >/dev/null 2>&1 || true
  fi
  sudo systemctl enable --now biorouter-xvfb.service >/dev/null
  sudo systemctl enable --now biorouter-headless.service >/dev/null
  sudo systemctl restart biorouter-headless.service
}

main() {
  require_ubuntu
  install_packages
  install_uv
  install_artifact
  ensure_env
  configure_systemd
  install_url_command
  repair_extension_paths
  repair_knowledge_paths
  start_services
  log "biorouter-xvfb: $(systemctl is-active biorouter-xvfb.service)"
  log "biorouter-headless: $(systemctl is-active biorouter-headless.service)"
  log "URL: $(/usr/local/bin/biorouter-headless-url)"
}

main "$@"
