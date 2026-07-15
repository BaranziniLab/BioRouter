#!/usr/bin/env bash
# Verify that the headless Linux artifact contains only app deliverables and
# does not accidentally package local user profiles, credential stores, or keys.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARTIFACT="$ROOT/dist/headless-linux-x64"
CREATE_TARBALL=false
name_hits=""
content_hits=""

log() { printf '\033[1;36m[headless-verify]\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m[headless-verify] %s\033[0m\n' "$*" >&2; exit 1; }

while [ "$#" -gt 0 ]; do
  case "$1" in
    --tar)
      CREATE_TARBALL=true
      ;;
    --artifact)
      shift
      ARTIFACT="${1:?missing path after --artifact}"
      ;;
    -h|--help)
      cat <<'USAGE'
Usage: scripts/verify-headless-artifact.sh [--artifact dist/headless-linux-x64] [--tar]

Checks that the headless Linux artifact has the expected shape and does not
contain personal profiles, local credential files, or obvious key material.

Options:
  --artifact PATH  Artifact directory to verify
  --tar            Also create dist/biorouter-headless-linux-x64.tar.gz
USAGE
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
  shift
done

[ -d "$ARTIFACT" ] || die "missing artifact directory: $ARTIFACT"
[ -d "$ARTIFACT/bin" ] || die "missing $ARTIFACT/bin"
[ -d "$ARTIFACT/web" ] || die "missing $ARTIFACT/web"
[ -f "$ARTIFACT/manifest.txt" ] || die "missing $ARTIFACT/manifest.txt"

for binary in biorouter biorouterd biorouter-headless; do
  path="$ARTIFACT/bin/$binary"
  [ -f "$path" ] || die "missing binary: $path"
  [ -x "$path" ] || die "binary is not executable: $path"
  if command -v file >/dev/null 2>&1; then
    file "$path" | grep -q 'ELF 64-bit.*x86-64' || die "binary is not Linux x86_64 ELF: $path"
  fi
done

[ -f "$ARTIFACT/web/index.html" ] || die "missing browser bundle entrypoint: $ARTIFACT/web/index.html"

log "checking artifact file names for profile or credential stores"
name_hits="$(mktemp)"
trap 'rm -f "$name_hits" "$content_hits"' EXIT
find "$ARTIFACT" -mindepth 1 -print \
  | sed "s#^$ARTIFACT/##" \
  | LC_ALL=C grep -E -i '(^|/)(\.aws|\.ssh|Library|Application Support|secrets\.ya?ml|config\.ya?ml|sessions\.db|openrouter\.env|wanjun\.gu_accessKeys\.csv)(/|$)|(^|/)(Users|home/ubuntu)(/|$)' \
  >"$name_hits" || true
if [ -s "$name_hits" ]; then
  sed -n '1,40p' "$name_hits" >&2
  die "artifact contains profile or credential-store paths"
fi

log "checking artifact contents for local paths and key-like material"
content_hits="$(mktemp)"
if command -v rg >/dev/null 2>&1; then
  rg -a -l --hidden --glob '!*.map' \
    -e 'sk-or-v1-[A-Za-z0-9_-]{16,}' \
    -e 'AKIA[0-9A-Z]{16}' \
    -e 'ASIA[0-9A-Z]{16}' \
    -e 'BEGIN (RSA |OPENSSH |EC )?PRIVATE KEY' \
    -e '/Users/wgu' \
    -e 'wanjun\.gu_accessKeys' \
    -e 'openrouter\.env' \
    -e 'aws_access_key_id[[:space:]]*=' \
    -e 'aws_secret_access_key[[:space:]]*=' \
    "$ARTIFACT" >"$content_hits" || true
else
  grep -R -I -E -l \
    'sk-or-v1-[A-Za-z0-9_-]{16,}|AKIA[0-9A-Z]{16}|ASIA[0-9A-Z]{16}|BEGIN (RSA |OPENSSH |EC )?PRIVATE KEY|/Users/wgu|wanjun\.gu_accessKeys|openrouter\.env|aws_access_key_id[[:space:]]*=|aws_secret_access_key[[:space:]]*=' \
    "$ARTIFACT" >"$content_hits" || true
fi
if [ -s "$content_hits" ]; then
  sed -n '1,40p' "$content_hits" >&2
  die "artifact contains local paths or key-like material; inspect these files without printing secrets"
fi

log "artifact manifest"
sed -n '1,20p' "$ARTIFACT/manifest.txt"

if [ "$CREATE_TARBALL" = true ]; then
  tarball="$ROOT/dist/biorouter-headless-linux-x64.tar.gz"
  log "creating $tarball"
  COPYFILE_DISABLE=1 tar --no-xattrs -C "$ROOT/dist" -czf "$tarball" "$(basename "$ARTIFACT")"
  shasum -a 256 "$tarball"
fi

log "artifact verified"
