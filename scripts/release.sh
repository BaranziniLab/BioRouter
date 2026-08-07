#!/usr/bin/env bash
#
# Biorouter cross-platform release automation.
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
#   linux-backend <ver>
#                     Rebuild just the linux x86_64 backend from scratch.
#   mac-arm64 <ver>   Package + sign + NOTARIZE the Apple Silicon .dmg.
#   mac-intel <ver>   Package + sign + NOTARIZE the Intel .dmg.
#   windows <ver>     Package the Windows .zip.
#   linux <ver>       Package the GUI .deb + .rpm.
#   cli-linux <ver>   Build the headless CLI-only .deb + .rpm.
#   headless-linux <ver>
#                     Build the browser-served headless Linux artifact.
#   mac-manifest <ver>
#                     Generate latest-mac.yml for electron-updater.
#   verify <ver>      Verify all release artifacts (arch, notarization, dmg format).
#   draft <ver>       Create a draft GitHub release with assets + notes.
#   publish <ver>     Publish a verified draft after native Windows smoke passes.
#   all <ver>         Run every build/verify phase and create the draft release.
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
#     flavored. Host-side browser, macOS, and headless builds must restore the
#     native optional dependencies before they run.
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

# The docker cross-compile recipes (linux/windows images, mingw winpthread
# linker wrap, LZMA_API_STATIC, the glibc-2.31 pin) live in ONE place so the
# release and the BR-70 `check-cross` CI gate can never drift apart.
# shellcheck source=scripts/cross-env.sh
. "$ROOT/scripts/cross-env.sh"

log()  { printf '\033[1;36m[release]\033[0m %s\n' "$*"; }
die()  { printf '\033[1;31m[release] ERROR:\033[0m %s\n' "$*" >&2; exit 1; }

need_version() { [ -n "${1:-}" ] || die "version required, e.g. scripts/release.sh $CMD 1.80.1"; }

activate_hermit() { set +u; source "$ROOT/bin/activate-hermit" >/dev/null 2>&1 || true; set -u; }

# Resolve notarization creds: env → macOS Keychain → gitignored notes file.
# The Keychain is the preferred store (encrypted at rest, no plaintext on disk).
# Seed it once (any local build agent can then read it without a prompt):
#   security add-generic-password -s biorouter-notarization -a APPLE_ID -w <id> -A -U
#   security add-generic-password -s biorouter-notarization -a APPLE_APP_SPECIFIC_PASSWORD -w <pw> -A -U
load_apple_creds() {
  if [ -z "${APPLE_ID:-}" ]; then
    APPLE_ID="$(security find-generic-password -s biorouter-notarization -a APPLE_ID -w 2>/dev/null || true)"
  fi
  if [ -z "${APPLE_APP_SPECIFIC_PASSWORD:-}" ]; then
    APPLE_APP_SPECIFIC_PASSWORD="$(security find-generic-password -s biorouter-notarization -a APPLE_APP_SPECIFIC_PASSWORD -w 2>/dev/null || true)"
  fi
  if [ -z "${APPLE_ID:-}" ] || [ -z "${APPLE_APP_SPECIFIC_PASSWORD:-}" ]; then
    [ -f "$NOTARY/APPLE_DEVELOPER_NOTES.md" ] || die "no APPLE_ID env, no Keychain item, and no $NOTARY/APPLE_DEVELOPER_NOTES.md"
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
    log "installing locked macOS desktop dependencies…"
    npm ci >/dev/null 2>&1
    npm rebuild macos-alias ds-store >/dev/null 2>&1 || true
    node -e "require('appdmg')" >/dev/null 2>&1 || die "appdmg still not loadable after npm ci"
  )
}

ensure_host_node_deps() {
  activate_hermit
  (
    cd "$DESK"
    log "installing locked host-native desktop dependencies…"
    npm ci >/dev/null
    node -e "require('rollup')" >/dev/null 2>&1 \
      || die "host-native Rollup dependency is unavailable"
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
    with open(path, encoding='utf-8') as f: data = json.load(f)
    fn(data)
    # ensure_ascii=False: these files contain literal em-dashes/ellipses (openapi.json
    # doc comments). Escaping them to \uXXXX churns hundreds of unrelated lines.
    # encoding pinned so a C/POSIX locale can't turn that into a UnicodeEncodeError.
    with open(path, 'w', encoding='utf-8') as f: json.dump(data, f, indent=2, ensure_ascii=False); f.write('\n')
setver('ui/desktop/package.json', lambda d: d.__setitem__('version', v))
setver('ui/desktop/openapi.json', lambda d: d['info'].__setitem__('version', v))
def lock(d):
    d['version'] = v
    d['packages'][''] ['version'] = v
setver('ui/desktop/package-lock.json', lock)
PY
  # README badge. Not one of the five for a long time, which is exactly why it
  # drifted to 1.87.2 while the tree was on 1.88.6 — it is the version most
  # people actually see, on the repo's front page, and nothing moved it.
  # `check-version-consistency.sh` now fails if this and Cargo.toml disagree.
  perl -0pi -e "s{(badge/version-)[0-9]+\.[0-9]+\.[0-9]+(-tan\.svg)}{\${1}$v\${2}}g" README.md
  perl -0pi -e "s{(alt=\"Version )[0-9]+\.[0-9]+\.[0-9]+(\")}{\${1}$v\${2}}g" README.md
  activate_hermit
  cargo update -p biorouter --precise "$v" >/dev/null 2>&1 || cargo check -q >/dev/null 2>&1 || true
  log "version is now: $(grep -m1 '^version' Cargo.toml)"
}

# ── backends ──────────────────────────────────────────────────────────────────
# The cross-compile images, toolchain env, mingw linker wrap and LZMA_API_STATIC
# now live in scripts/cross-env.sh (sourced above) — the SAME recipe the BR-70
# `check-cross` CI gate uses, so what the gate checks is exactly what ships.

# Linux x86_64 backend (biorouterd + biorouter). Extracted so it can be re-run
# on its own. Cleans the target dir first to force a from-scratch compile
# against the pinned glibc (cached objects would keep stale symbol versions).
cmd_linux-backend() {
  ensure_docker
  log "cross-compiling linux-gnu backend (docker, $LINUX_RUST_IMG)"
  rm -rf "$ROOT/target/x86_64-unknown-linux-gnu/release/biorouter" \
         "$ROOT/target/x86_64-unknown-linux-gnu/release/biorouterd"
  run_cross_release \
    cross_linux \
    biorouter-linux-release-target \
    "cargo build --release --bin biorouterd --bin biorouter" \
    "mkdir -p /usr/src/myapp/target/x86_64-unknown-linux-gnu/release && \
     cp -f /cross-target/x86_64-unknown-linux-gnu/release/biorouter \
           /cross-target/x86_64-unknown-linux-gnu/release/biorouterd \
           /usr/src/myapp/target/x86_64-unknown-linux-gnu/release/"
  log "linux backend compiled"
}

run_cross_release() { # <cross function> <target volume> <cargo command> <post command>
  local cross_fn="$1" target_volume="$2" cargo_cmd="$3" post_cmd="$4" rc=0
  docker volume rm -f "$target_volume" >/dev/null 2>&1 || true
  docker volume create "$target_volume" >/dev/null
  (
    export CROSS_TARGET_MOUNT="$target_volume"
    "$cross_fn" "$cargo_cmd" /cross-target "$post_cmd"
  ) || rc=$?
  docker volume rm -f "$target_volume" >/dev/null 2>&1 || true
  return "$rc"
}

cmd_backends() {
  local v="$1"
  activate_hermit
  log "compiling mac arm64 release backend"
  cargo build --release
  log "compiling mac x64 release backend"
  cargo build --release --target x86_64-apple-darwin

  ensure_docker
  log "cross-compiling windows-gnu backend (docker)"
  run_cross_release \
    cross_windows \
    biorouter-windows-release-target \
    "cargo build --release --bin biorouterd --bin biorouter" \
    "mkdir -p /usr/src/myapp/target/x86_64-pc-windows-gnu/release && \
     cp -f /cross-target/x86_64-pc-windows-gnu/release/biorouter.exe \
           /cross-target/x86_64-pc-windows-gnu/release/biorouterd.exe \
           /usr/src/myapp/target/x86_64-pc-windows-gnu/release/ && \
     $WIN_DLL_STAGE"

  cmd_linux-backend
  log "all 4 backends compiled"
}

stage_bin() { # <src-dir> <ext>
  rm -rf "$DESK/src/bin"; mkdir -p "$DESK/src/bin"
  cp -p "$1/biorouter${2:-}" "$1/biorouterd${2:-}" "$DESK/src/bin/"
}

# ── mac packaging (sign + notarize, Node 24 via hermit) ───────────────────────
cmd_mac-arm64() {
  local v="$1"; activate_hermit; load_apple_creds; ensure_mac_dmg_deps
  ls /Volumes/Biorouter* >/dev/null 2>&1 && { umount /Volumes/Biorouter* 2>/dev/null || true; }
  stage_bin "$ROOT/target/release"
  log "building + notarizing macOS arm64 dmg"
  ( cd "$DESK" && APPLE_ID="$APPLE_ID" APPLE_APP_SPECIFIC_PASSWORD="$APPLE_APP_SPECIFIC_PASSWORD" npm run bundle:default )
  log "arm64 dmg: $DESK/out/make/Biorouter-$v-arm64.dmg"
}

cmd_mac-intel() {
  local v="$1"; activate_hermit; load_apple_creds; ensure_mac_dmg_deps
  ls /Volumes/Biorouter* >/dev/null 2>&1 && { umount /Volumes/Biorouter* 2>/dev/null || true; }
  stage_bin "$ROOT/target/x86_64-apple-darwin/release"
  log "building + notarizing macOS Intel dmg"
  ( cd "$DESK" && APPLE_ID="$APPLE_ID" APPLE_APP_SPECIFIC_PASSWORD="$APPLE_APP_SPECIFIC_PASSWORD" npm run bundle:intel )
  log "x64 dmg: $DESK/out/make/Biorouter-$v-x64.dmg"
}

# ── windows packaging (host forge, Node 24) ───────────────────────────────────
cmd_windows() {
  local v="$1"; activate_hermit; ensure_host_node_deps
  local WR="$ROOT/target/x86_64-pc-windows-gnu/release"
  [ -f "$WR/biorouterd.exe" ] || die "windows backend missing — run: scripts/release.sh backends $v"
  rm -rf "$DESK/src/bin"; mkdir -p "$DESK/src/bin"
  cp -f "$WR/biorouterd.exe" "$WR/biorouter.exe" "$WR"/*.dll "$DESK/src/bin/"
  log "packaging Windows zip"
  ( cd "$DESK" && npm run bundle:windows )
  log "windows zip: $DESK/out/make/zip/win32/x64/Biorouter-win32-x64-$v.zip"
}

# ── linux packaging (fully dockerized; run LAST — corrupts node_modules) ───────
cmd_linux() {
  local v="$1"; ensure_docker
  [ -f "$ROOT/target/x86_64-unknown-linux-gnu/release/biorouterd" ] || die "linux backend missing — run: scripts/release.sh backends $v"
  log "packaging Linux deb + rpm (docker)"
  docker volume create biorouter-linux-npm-cache >/dev/null 2>&1 || true
  docker run --rm --platform linux/amd64 -v "$ROOT":/ws -v biorouter-linux-npm-cache:/root/.npm \
    node:24-bookworm bash /ws/ui/desktop/scripts/build-linux-deb.sh
  log "deb: $DESK/out/make/deb/x64/biorouter_${v}_amd64.deb"
  log "rpm: $DESK/out/make/rpm/x64/Biorouter-$v-1.x86_64.rpm"
  log "NOTE: node_modules is now Linux-flavored — run 'cd ui/desktop && npm ci' before any further mac build."
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

# ── Headless browser Linux artifact ───────────────────────────────────────────
# Independent of the Electron GUI packages. Produces the server/browser bundle
# used for Debian/Ubuntu deployments and verifies that no local profiles or
# credential material were packaged.
cmd_headless-linux() {
  local v="$1"; ensure_docker; ensure_host_node_deps
  log "building headless Linux browser artifact"
  "$ROOT/scripts/package-headless-linux.sh"
  local tarball="$ROOT/dist/biorouter-headless-linux-x64.tar.gz"
  [ -f "$tarball" ] || die "headless artifact missing: $tarball"
  log "headless tarball: $tarball ($(du -h "$tarball" | cut -f1))"
}

# ── verify ────────────────────────────────────────────────────────────────────
cmd_verify() {
  local v="$1" ok=1
  "$ROOT/scripts/check-brand-consistency.sh"
  local arm="$DESK/out/make/Biorouter-$v-arm64.dmg"
  local x64="$DESK/out/make/Biorouter-$v-x64.dmg"
  local win="$DESK/out/make/zip/win32/x64/Biorouter-win32-x64-$v.zip"
  local deb="$DESK/out/make/deb/x64/biorouter_${v}_amd64.deb"
  local rpm="$DESK/out/make/rpm/x64/Biorouter-$v-1.x86_64.rpm"
  local clideb="$ROOT/dist/cli/biorouter-cli_${v}_amd64.deb"
  local clirpm="$ROOT/dist/cli/biorouter-cli-${v}-1.x86_64.rpm"
  local headless="$ROOT/dist/biorouter-headless-linux-x64.tar.gz"
  local armzip="$DESK/out/make/$ARM64_ZIP_REL/Biorouter-darwin-arm64-$v.zip"
  local x64zip="$DESK/out/make/$X64_ZIP_REL/Biorouter-darwin-x64-$v.zip"
  for f in "$arm" "$x64" "$armzip" "$x64zip" "$win" "$deb" "$rpm" "$clideb" "$clirpm" "$headless"; do
    [ -f "$f" ] && log "present: $(basename "$f") ($(du -h "$f" | cut -f1))" || { printf 'MISSING: %s\n' "$f"; ok=0; }
  done
  "$ROOT/scripts/verify-headless-artifact.sh" >/dev/null || ok=0
  # The electron-updater manifest is generated at publish time; verify it if
  # already present (and that it references both arch zips).
  local yml="$DESK/out/make/latest-mac.yml"
  if [ -f "$yml" ]; then
    grep -q "Biorouter-darwin-arm64-$v.zip" "$yml" && grep -q "Biorouter-darwin-x64-$v.zip" "$yml" \
      && log "latest-mac.yml references both arch zips ✓" || { echo "latest-mac.yml missing an arch zip"; ok=0; }
  fi
  if [ -d "$DESK/out/Biorouter-darwin-arm64/Biorouter.app" ]; then
    log "arm64 gatekeeper: $(spctl --assess --type execute --verbose "$DESK/out/Biorouter-darwin-arm64/Biorouter.app" 2>&1 | tr '\n' ' ')"
    xcrun stapler validate "$DESK/out/Biorouter-darwin-arm64/Biorouter.app" >/dev/null 2>&1 && log "arm64 app stapled ✓" || { echo "arm64 NOT stapled"; ok=0; }
  fi
  if [ -d "$DESK/out/Biorouter-darwin-x64/Biorouter.app" ]; then
    file "$DESK/out/Biorouter-darwin-x64/Biorouter.app/Contents/Resources/bin/biorouterd" | grep -q x86_64 && log "intel bundled binary is x86_64 ✓" || { echo "intel binary WRONG ARCH"; ok=0; }
    xcrun stapler validate "$DESK/out/Biorouter-darwin-x64/Biorouter.app" >/dev/null 2>&1 && log "intel app stapled ✓" || { echo "intel NOT stapled"; ok=0; }
  fi
  "$ROOT/scripts/smoke-test-release-artifacts.sh" "$v" || ok=0
  [ "$ok" = 1 ] || die "verification failed"
  log "all artifacts verified"
}

# ── electron-updater macOS manifest ───────────────────────────────────────────
# latest-mac.yml is what lets the in-app "Restart & Update" button do a silent,
# one-click, in-place update on macOS (Squirrel.Mac installs from the signed
# maker-zip archives). Without it electron-updater 404s and clients fall back to
# the assisted "download to ~/Downloads" path. Re-runnable; needs both mac
# zips present (produced by `mac-arm64` + `mac-intel`).
ARM64_ZIP_REL="zip/darwin/arm64"
X64_ZIP_REL="zip/darwin/x64"
cmd_mac-manifest() {
  local v="$1"; activate_hermit
  local armzip="$DESK/out/make/$ARM64_ZIP_REL/Biorouter-darwin-arm64-$v.zip"
  local x64zip="$DESK/out/make/$X64_ZIP_REL/Biorouter-darwin-x64-$v.zip"
  [ -f "$armzip" ] || die "mac arm64 zip missing — run: scripts/release.sh mac-arm64 $v"
  [ -f "$x64zip" ] || die "mac x64 zip missing — run: scripts/release.sh mac-intel $v"
  log "generating latest-mac.yml for v$v"
  ( cd "$DESK" && node scripts/generate-update-manifests.js \
      --version "$v" --arm64-zip "$armzip" --x64-zip "$x64zip" --out "$DESK/out/make" )
  log "latest-mac.yml: $DESK/out/make/latest-mac.yml"
}

# ── draft + publish ───────────────────────────────────────────────────────────
release_assets() {
  local v="$1"
  printf '%s\n' \
    "$DESK/out/make/Biorouter-$v-arm64.dmg" \
    "$DESK/out/make/Biorouter-$v-x64.dmg" \
    "$DESK/out/make/$ARM64_ZIP_REL/Biorouter-darwin-arm64-$v.zip" \
    "$DESK/out/make/$X64_ZIP_REL/Biorouter-darwin-x64-$v.zip" \
    "$DESK/out/make/latest-mac.yml" \
    "$DESK/out/make/zip/win32/x64/Biorouter-win32-x64-$v.zip" \
    "$DESK/out/make/deb/x64/biorouter_${v}_amd64.deb" \
    "$DESK/out/make/rpm/x64/Biorouter-$v-1.x86_64.rpm" \
    "$ROOT/dist/cli/biorouter-cli_${v}_amd64.deb" \
    "$ROOT/dist/cli/biorouter-cli-${v}-1.x86_64.rpm" \
    "$ROOT/dist/biorouter-headless-linux-x64.tar.gz"
}

cmd_draft() {
  local v="$1"
  local notes="$ROOT/docs/releases/notes/v$v.md"
  [ -f "$notes" ] || die "release notes missing: $notes"
  cmd_mac-manifest "$v"
  local assets=()
  while IFS= read -r asset; do
    [ -f "$asset" ] || die "release asset missing: $asset"
    assets+=("$asset")
  done < <(release_assets "$v")
  log "creating draft GitHub release v$v"
  gh release create "v$v" --draft --target main --title "Biorouter v$v" \
    --notes-file "$notes" "${assets[@]}"
  log "draft ready: $(gh release view "v$v" --json url --jq .url)"
}

cmd_publish() {
  local v="$1"
  cmd_verify "$v"
  local is_draft windows_smoke
  is_draft="$(gh release view "v$v" --json isDraft --jq .isDraft 2>/dev/null || true)"
  [ "$is_draft" = true ] || die "v$v must exist as a draft release before publication"
  windows_smoke="$(gh run list --workflow release-artifact-smoke.yml --limit 30 \
    --json displayTitle,conclusion \
    --jq "[.[] | select(.displayTitle == \"Release artifact smoke v$v\" and .conclusion == \"success\")] | length")"
  [ "$windows_smoke" -gt 0 ] || die "native Windows release smoke has not passed for v$v"
  gh release edit "v$v" --draft=false
  log "published: $(gh release view "v$v" --json url --jq .url)"
}

cmd_all() {
  local v="$1"
  cmd_bump "$v"; cmd_backends "$v"
  cmd_mac-arm64 "$v"; cmd_mac-intel "$v"; cmd_windows "$v"; cmd_linux "$v"
  cmd_cli-linux "$v"                                                    # headless CLI deb/rpm
  cmd_headless-linux "$v"                                                # browser-served headless Linux
  ( cd "$DESK" && npm ci >/dev/null 2>&1 )
  cmd_verify "$v"; cmd_draft "$v"
  log "draft created; run the native Windows smoke workflow, then: scripts/release.sh publish $v"
}

# ── version resolution ───────────────────────────────────────────────────────
# Accepts a literal `X.Y.Z` (optionally `vX.Y.Z`, since that is how the tags are
# spelled and it is the obvious thing to paste), or one of three keywords that
# compute the next version from the tree's current one:
#
#   major        1.88.6 → 2.0.0     the FIRST number; resets the other two
#   minor        1.88.6 → 1.89.0    the SECOND number; resets the third
#   patch        1.88.6 → 1.88.7    the THIRD number
#
# `minor-minor` is accepted as an alias for `patch`, because "the minor minor
# version" is how the third number gets described in practice and a script that
# rejects the user's own vocabulary is a script people stop using.
current_version() {
  perl -0ne 'print $1 if /\[workspace\.package\].*?version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"/s' Cargo.toml
}

resolve_version() {
  local want="$1" cur part
  cur="$(current_version)"
  [ -n "$cur" ] || die "could not read the current version from Cargo.toml [workspace.package]"

  case "$want" in
    major|minor|patch|minor-minor)
      part="$want"
      [ "$part" = minor-minor ] && part=patch
      IFS=. read -r a b c <<<"$cur"
      case "$part" in
        major) echo "$((a + 1)).0.0" ;;
        minor) echo "$a.$((b + 1)).0" ;;
        patch) echo "$a.$b.$((c + 1))" ;;
      esac
      ;;
    v[0-9]*.[0-9]*.[0-9]*|[0-9]*.[0-9]*.[0-9]*)
      local v="${want#v}"
      [[ "$v" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] \
        || die "malformed version '$want' — expected X.Y.Z, or one of: major minor patch"
      # Going backwards silently is a real hazard: electron-updater compares
      # versions, so a lower number ships an update clients will refuse and a
      # `latest-mac.yml` that disagrees with its own assets.
      if [ "$(printf '%s\n%s\n' "$cur" "$v" | sort -V | tail -1)" = "$cur" ] && [ "$v" != "$cur" ]; then
        die "refusing to bump BACKWARDS: $cur → $v. Pass the version explicitly again if this is deliberate — but check the update feed first."
      fi
      echo "$v"
      ;;
    *)
      die "unrecognised version '$want' — expected X.Y.Z, or one of: major minor patch (minor-minor = patch)"
      ;;
  esac
}

CMD="${1:-}"; VER="${2:-}"
case "$CMD" in
  bump|all)
    need_version "$VER"
    RESOLVED="$(resolve_version "$VER")"
    if [ "$RESOLVED" != "$VER" ]; then
      log "resolved '$VER' → $RESOLVED (current: $(current_version))"
    fi
    "cmd_${CMD}" "$RESOLVED"
    [ "$CMD" = bump ] && log "later phases take this explicitly, e.g. scripts/release.sh backends $RESOLVED"
    ;;
  backends|linux-backend|mac-arm64|mac-intel|mac-manifest|windows|linux|cli-linux|headless-linux|verify|draft|publish)
    need_version "$VER"
    # Keywords are deliberately REFUSED here. These phases run against a tree
    # that `bump` has already rewritten, so `minor` would resolve against the
    # NEW current version and compute a version one step too far — building
    # 1.90.0 artifacts for a 1.89.0 tree, with nothing to catch it until verify.
    case "$VER" in
      major|minor|patch|minor-minor)
        die "'$VER' is only valid for 'bump' and 'all'. This phase needs the explicit version the tree is already at: $(current_version)" ;;
    esac
    "cmd_${CMD}" "$VER" ;;
  *) die "usage: scripts/release.sh {bump|backends|linux-backend|mac-arm64|mac-intel|mac-manifest|windows|linux|cli-linux|headless-linux|verify|draft|publish|all} <version|major|minor|patch>" ;;
esac
