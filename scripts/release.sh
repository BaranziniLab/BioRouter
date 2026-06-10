#!/usr/bin/env bash
#
# BioRouter cross-platform release automation.
#
# Encodes the full release pipeline so a human OR an agent can cut a signed,
# notarized, multi-platform release reproducibly. Each phase is a separate
# subcommand so a workflow can run/verify them independently and resume.
#
#   scripts/release.sh <command> <version>
#
# Commands:
#   bump <ver>        Bump version in the 5 release files + refresh Cargo.lock.
#   backends <ver>    Compile release backends for all 4 targets
#                     (mac arm64, mac x64, windows-gnu, linux-gnu).
#   mac-arm64 <ver>   Package + sign + NOTARIZE the Apple Silicon .dmg.
#   mac-intel <ver>   Package + sign + NOTARIZE the Intel .dmg.
#   windows <ver>     Package the Windows .zip.
#   linux <ver>       Package the Linux .deb + .rpm.
#   verify <ver>      Verify all 5 artifacts (arch, notarization, dmg format).
#   publish <ver>     Create the GitHub release with the 5 assets + notes.
#   all <ver>         Run every phase in order (bump → … → publish).
#
# Hard-won invariants (see CLAUDE.md for the long version):
#   * The macOS .dmg maker (macos-alias native module) only builds under
#     Node 24 — use hermit's node, NOT a newer Homebrew node. All packaging
#     runs under `source bin/activate-hermit`.
#   * The windows-gnu / linux-gnu cross builds must run with the SYSTEM docker,
#     not hermit's docker shim (which points at the wrong socket). We invoke
#     `docker` directly here rather than via `just`.
#   * aws-lc-sys (AWS SDK / rustls) needs winpthread appended AFTER the rlibs on
#     the mingw link line; lzma-sys (xz2, .brkb path) needs LZMA_API_STATIC=1 so
#     it statically builds bundled liblzma instead of finding the host one.
#     Both are applied below for the cross targets (and live in the Justfile).
#   * Every bundle writes ui/desktop/src/bin/ and clobbers the others — phases
#     stage the correct binaries and run strictly one platform at a time.
#   * The Linux docker package runs `npm ci` and leaves node_modules Linux-
#     flavored, so it MUST be the last package phase; `verify`/`publish` and any
#     later mac build need `cd ui/desktop && rm -rf node_modules && npm install`.
#   * Notarization credentials are read from notarization/APPLE_DEVELOPER_NOTES.md
#     (gitignored). Override via APPLE_ID / APPLE_APP_SPECIFIC_PASSWORD env vars.
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
DESK="$ROOT/ui/desktop"
NOTARY="$ROOT/notarization"
SIGN_IDENTITY="Developer ID Application: University of California at San Francisco (F3YYBXAFJ8)"
TEAM_ID="F3YYBXAFJ8"

log()  { printf '\033[1;36m[release]\033[0m %s\n' "$*"; }
die()  { printf '\033[1;31m[release] ERROR:\033[0m %s\n' "$*" >&2; exit 1; }

need_version() { [ -n "${1:-}" ] || die "version required, e.g. scripts/release.sh $CMD 1.80.1"; }

activate_hermit() { set +u; source "$ROOT/bin/activate-hermit" >/dev/null 2>&1 || true; set -u; }

# Pull notarization creds from the gitignored notes file unless already in env.
load_apple_creds() {
  if [ -z "${APPLE_ID:-}" ] || [ -z "${APPLE_APP_SPECIFIC_PASSWORD:-}" ]; then
    [ -f "$NOTARY/APPLE_DEVELOPER_NOTES.md" ] || die "no APPLE_ID env and no $NOTARY/APPLE_DEVELOPER_NOTES.md"
    APPLE_ID="$(grep -iE 'Apple ID \(notarization\)' "$NOTARY/APPLE_DEVELOPER_NOTES.md" | grep -oE '`[^`]+`' | tr -d '`' | head -1)"
    APPLE_APP_SPECIFIC_PASSWORD="$(grep -iE 'App-specific password' "$NOTARY/APPLE_DEVELOPER_NOTES.md" | grep -oE '`[^`]+`' | tr -d '`' | head -1)"
  fi
  [ -n "$APPLE_ID" ] && [ -n "$APPLE_APP_SPECIFIC_PASSWORD" ] || die "could not resolve Apple notarization credentials"
  export APPLE_ID APPLE_APP_SPECIFIC_PASSWORD
}

ensure_docker() {
  docker info >/dev/null 2>&1 && return 0
  log "starting Docker Desktop…"; open -a Docker || true
  for _ in $(seq 1 60); do docker info >/dev/null 2>&1 && return 0; sleep 5; done
  die "Docker daemon not reachable"
}

# The macOS .dmg maker needs the darwin-only `appdmg` (+ its native deps
# macos-alias / ds-store). A prior Linux docker package or a partial reinstall
# can drop them, and they must be (re)built against hermit's Node. Ensure they
# are present and loadable before any dmg build, otherwise the maker dies with
# "Cannot find module 'appdmg'" / a NODE_MODULE_VERSION mismatch.
ensure_mac_dmg_deps() {
  ( cd "$DESK"
    if ! node -e "require.resolve('appdmg')" >/dev/null 2>&1; then
      log "installing macOS dmg deps (appdmg)…"; npm install >/dev/null 2>&1
    fi
    npm rebuild macos-alias ds-store >/dev/null 2>&1 || true
    node -e "require('appdmg')" >/dev/null 2>&1 || die "appdmg still not loadable — run: (cd ui/desktop && rm -rf node_modules && npm install)"
  )
}

# ── bump ────────────────────────────────────────────────────────────────────
cmd_bump() {
  local v="$1"
  log "bumping version → $v"
  # Cargo workspace package version (the line under [workspace.package]).
  perl -0pi -e "s/(\[workspace\.package\][^\[]*?version = \")[0-9.]+(\")/\${1}$v\${2}/s" Cargo.toml
  # The three desktop JSON files (package.json, package-lock.json x2, openapi.json).
  python3 - "$v" <<'PY'
import json, sys
v = sys.argv[1]
def setver(path, fn):
    with open(path) as f: data = json.load(f)
    fn(data)
    with open(path, 'w') as f: json.dump(data, f, indent=2); f.write('\n')
setver('ui/desktop/package.json', lambda d: d.__setitem__('version', v))
setver('ui/desktop/openapi.json', lambda d: d['info'].__setitem__('version', v))
def lock(d):
    d['version'] = v
    d['packages'][''] ['version'] = v
setver('ui/desktop/package-lock.json', lock)
PY
  activate_hermit
  cargo update -p biorouter --precise "$v" >/dev/null 2>&1 || cargo check -q >/dev/null 2>&1 || true
  log "version is now: $(grep -m1 '^version' Cargo.toml)"
}

# ── backends ──────────────────────────────────────────────────────────────────
WIN_LINKER_WRAP='printf "#!/bin/sh\nexec x86_64-w64-mingw32-gcc \"\$@\" -lpthread -lwinpthread\n" > /usr/local/bin/winpthread-gcc && chmod +x /usr/local/bin/winpthread-gcc'

cmd_backends() {
  local v="$1"
  activate_hermit
  log "compiling mac arm64 release backend"
  cargo build --release
  log "compiling mac x64 release backend"
  cargo build --release --target x86_64-apple-darwin

  ensure_docker
  log "cross-compiling windows-gnu backend (docker)"
  docker volume create biorouter-windows-cache >/dev/null 2>&1 || true
  docker run --rm -v "$ROOT":/usr/src/myapp -v biorouter-windows-cache:/usr/local/cargo/registry \
    -w /usr/src/myapp rust:latest sh -c "
      rustup target add x86_64-pc-windows-gnu && apt-get update &&
      apt-get install -y mingw-w64 protobuf-compiler cmake &&
      $WIN_LINKER_WRAP &&
      export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc CXX_x86_64_pc_windows_gnu=x86_64-w64-mingw32-g++ \
             AR_x86_64_pc_windows_gnu=x86_64-w64-mingw32-ar \
             CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=/usr/local/bin/winpthread-gcc \
             LZMA_API_STATIC=1 PKG_CONFIG_ALLOW_CROSS=1 PROTOC=/usr/bin/protoc PATH=/usr/bin:\$PATH &&
      cargo build --release --target x86_64-pc-windows-gnu --bin biorouterd --bin biorouter &&
      GCC_DIR=\$(ls -d /usr/lib/gcc/x86_64-w64-mingw32/*/ | head -n 1) &&
      cp \$GCC_DIR/libstdc++-6.dll \$GCC_DIR/libgcc_s_seh-1.dll \
         /usr/x86_64-w64-mingw32/lib/libwinpthread-1.dll \
         /usr/src/myapp/target/x86_64-pc-windows-gnu/release/"

  log "cross-compiling linux-gnu backend (docker)"
  docker volume create biorouter-linux-cache >/dev/null 2>&1 || true
  docker run --rm -v "$ROOT":/usr/src/myapp -v biorouter-linux-cache:/usr/local/cargo/registry \
    -w /usr/src/myapp rust:latest sh -c '
      rustup target add x86_64-unknown-linux-gnu && dpkg --add-architecture amd64 && apt-get update -q &&
      apt-get install -y --no-install-recommends gcc-x86-64-linux-gnu g++-x86-64-linux-gnu protobuf-compiler cmake libxcb1-dev:amd64 libbz2-dev:amd64 &&
      export CC_x86_64_unknown_linux_gnu=x86_64-linux-gnu-gcc CXX_x86_64_unknown_linux_gnu=x86_64-linux-gnu-g++ \
             AR_x86_64_unknown_linux_gnu=x86_64-linux-gnu-ar \
             CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc \
             LZMA_API_STATIC=1 PKG_CONFIG_ALLOW_CROSS=1 \
             PKG_CONFIG_PATH_x86_64_unknown_linux_gnu=/usr/lib/x86_64-linux-gnu/pkgconfig PROTOC=/usr/bin/protoc &&
      cargo build --release --target x86_64-unknown-linux-gnu --bin biorouterd --bin biorouter'
  log "all 4 backends compiled"
}

stage_bin() { # <src-dir> <ext>
  rm -rf "$DESK/src/bin"; mkdir -p "$DESK/src/bin"
  cp -p "$1/biorouter${2:-}" "$1/biorouterd${2:-}" "$DESK/src/bin/"
}

# ── mac packaging (sign + notarize, Node 24 via hermit) ───────────────────────
cmd_mac-arm64() {
  local v="$1"; activate_hermit; load_apple_creds; ensure_mac_dmg_deps
  ls /Volumes/BioRouter* >/dev/null 2>&1 && { umount /Volumes/BioRouter* 2>/dev/null || true; }
  stage_bin "$ROOT/target/release"
  log "building + notarizing macOS arm64 dmg"
  ( cd "$DESK" && APPLE_ID="$APPLE_ID" APPLE_APP_SPECIFIC_PASSWORD="$APPLE_APP_SPECIFIC_PASSWORD" npm run bundle:default )
  log "arm64 dmg: $DESK/out/make/BioRouter-$v-arm64.dmg"
}

cmd_mac-intel() {
  local v="$1"; activate_hermit; load_apple_creds; ensure_mac_dmg_deps
  ls /Volumes/BioRouter* >/dev/null 2>&1 && { umount /Volumes/BioRouter* 2>/dev/null || true; }
  stage_bin "$ROOT/target/x86_64-apple-darwin/release"
  log "building + notarizing macOS Intel dmg"
  ( cd "$DESK" && APPLE_ID="$APPLE_ID" APPLE_APP_SPECIFIC_PASSWORD="$APPLE_APP_SPECIFIC_PASSWORD" npm run bundle:intel )
  log "x64 dmg: $DESK/out/make/BioRouter-$v-x64.dmg"
}

# ── windows packaging (host forge, Node 24) ───────────────────────────────────
cmd_windows() {
  local v="$1"; activate_hermit
  local WR="$ROOT/target/x86_64-pc-windows-gnu/release"
  [ -f "$WR/biorouterd.exe" ] || die "windows backend missing — run: scripts/release.sh backends $v"
  rm -rf "$DESK/src/bin"; mkdir -p "$DESK/src/bin"
  cp -f "$WR/biorouterd.exe" "$WR/biorouter.exe" "$WR"/*.dll "$DESK/src/bin/"
  log "packaging Windows zip"
  ( cd "$DESK" && npm run bundle:windows )
  log "windows zip: $DESK/out/make/zip/win32/x64/BioRouter-win32-x64-$v.zip"
}

# ── linux packaging (fully dockerized; run LAST — corrupts node_modules) ───────
cmd_linux() {
  local v="$1"; ensure_docker
  [ -f "$ROOT/target/x86_64-unknown-linux-gnu/release/biorouterd" ] || die "linux backend missing — run: scripts/release.sh backends $v"
  log "packaging Linux deb + rpm (docker)"
  docker volume create biorouter-linux-npm-cache >/dev/null 2>&1 || true
  docker run --rm --platform linux/amd64 -v "$ROOT":/ws -v biorouter-linux-npm-cache:/root/.npm \
    node:20-bookworm bash /ws/ui/desktop/scripts/build-linux-deb.sh
  log "deb: $DESK/out/make/deb/x64/biorouter_${v}_amd64.deb"
  log "rpm: $DESK/out/make/rpm/x64/BioRouter-$v-1.x86_64.rpm"
  log "NOTE: node_modules is now Linux-flavored — run 'cd ui/desktop && rm -rf node_modules && npm install' before any further mac build."
}

# ── CLI-only Linux packages (deb + rpm; headless biorouter + biorouterd) ───────
# Independent of the GUI packaging — does NOT corrupt node_modules. Builds and
# smoke-tests both packages in clean containers.
cmd_cli-linux() {
  local v="$1"; ensure_docker
  [ -f "$ROOT/target/x86_64-unknown-linux-gnu/release/biorouter" ] || die "linux backend missing — run: scripts/release.sh backends $v"
  log "building CLI-only Linux packages (deb + rpm)"
  bash "$ROOT/scripts/build-cli-linux-packages.sh" "$v"
  log "cli deb: $ROOT/dist/cli/biorouter-cli_${v}_amd64.deb"
  log "cli rpm: $ROOT/dist/cli/biorouter-cli-${v}-1.x86_64.rpm"
}

# ── verify ────────────────────────────────────────────────────────────────────
cmd_verify() {
  local v="$1" ok=1
  local arm="$DESK/out/make/BioRouter-$v-arm64.dmg"
  local x64="$DESK/out/make/BioRouter-$v-x64.dmg"
  local win="$DESK/out/make/zip/win32/x64/BioRouter-win32-x64-$v.zip"
  local deb="$DESK/out/make/deb/x64/biorouter_${v}_amd64.deb"
  local rpm="$DESK/out/make/rpm/x64/BioRouter-$v-1.x86_64.rpm"
  local clideb="$ROOT/dist/cli/biorouter-cli_${v}_amd64.deb"
  local clirpm="$ROOT/dist/cli/biorouter-cli-${v}-1.x86_64.rpm"
  for f in "$arm" "$x64" "$win" "$deb" "$rpm" "$clideb" "$clirpm"; do
    [ -f "$f" ] && log "present: $(basename "$f") ($(du -h "$f" | cut -f1))" || { printf 'MISSING: %s\n' "$f"; ok=0; }
  done
  if [ -d "$DESK/out/BioRouter-darwin-arm64/BioRouter.app" ]; then
    log "arm64 gatekeeper: $(spctl --assess --type execute --verbose "$DESK/out/BioRouter-darwin-arm64/BioRouter.app" 2>&1 | tr '\n' ' ')"
    xcrun stapler validate "$DESK/out/BioRouter-darwin-arm64/BioRouter.app" >/dev/null 2>&1 && log "arm64 app stapled ✓" || { echo "arm64 NOT stapled"; ok=0; }
  fi
  if [ -d "$DESK/out/BioRouter-darwin-x64/BioRouter.app" ]; then
    file "$DESK/out/BioRouter-darwin-x64/BioRouter.app/Contents/Resources/bin/biorouterd" | grep -q x86_64 && log "intel bundled binary is x86_64 ✓" || { echo "intel binary WRONG ARCH"; ok=0; }
    xcrun stapler validate "$DESK/out/BioRouter-darwin-x64/BioRouter.app" >/dev/null 2>&1 && log "intel app stapled ✓" || { echo "intel NOT stapled"; ok=0; }
  fi
  [ "$ok" = 1 ] || die "verification failed"
  log "all artifacts verified"
}

# ── publish ───────────────────────────────────────────────────────────────────
cmd_publish() {
  local v="$1"
  local notes="$ROOT/docs/release-notes/v$v.md"
  [ -f "$notes" ] || die "release notes missing: $notes"
  log "creating GitHub release v$v"
  gh release create "v$v" --title "BioRouter v$v" --notes-file "$notes" \
    "$DESK/out/make/BioRouter-$v-arm64.dmg" \
    "$DESK/out/make/BioRouter-$v-x64.dmg" \
    "$DESK/out/make/zip/win32/x64/BioRouter-win32-x64-$v.zip" \
    "$DESK/out/make/deb/x64/biorouter_${v}_amd64.deb" \
    "$DESK/out/make/rpm/x64/BioRouter-$v-1.x86_64.rpm" \
    "$ROOT/dist/cli/biorouter-cli_${v}_amd64.deb" \
    "$ROOT/dist/cli/biorouter-cli-${v}-1.x86_64.rpm"
  log "published: $(gh release view "v$v" --json url --jq .url)"
}

cmd_all() {
  local v="$1"
  cmd_bump "$v"; cmd_backends "$v"
  cmd_mac-arm64 "$v"; cmd_mac-intel "$v"; cmd_windows "$v"; cmd_linux "$v"
  cmd_cli-linux "$v"                                                    # headless CLI deb/rpm
  ( cd "$DESK" && rm -rf node_modules && npm install >/dev/null 2>&1 )  # un-Linux node_modules
  cmd_verify "$v"; cmd_publish "$v"
}

CMD="${1:-}"; VER="${2:-}"
case "$CMD" in
  bump|backends|mac-arm64|mac-intel|windows|linux|cli-linux|verify|publish|all)
    need_version "$VER"; "cmd_${CMD}" "$VER" ;;
  *) die "usage: scripts/release.sh {bump|backends|mac-arm64|mac-intel|windows|linux|cli-linux|verify|publish|all} <version>" ;;
esac
