# BR-70 — Cross-platform CI verification gate

**Lens:** R (robustness). **Status:** proposed; not implemented.
**Depends on / protects:** BR-20 (catastrophic denylist), BR-21 (policy engine),
BR-37 (background-job process-group kill), BR-64 (OS sandbox). Each of those
shipped `#[cfg]`-gated code on a platform **no machine in the project has ever
compiled outside a release**.

---

## Problem (grounded in code, with file:line)

### 1. There is no Rust CI. At all.

`.github/workflows/` contains exactly two workflows:

| File | What it does |
|------|--------------|
| `check-commits.yml` | greps commit messages for AI `Co-Authored-By:` trailers |
| `deploy-landing.yml` | publishes `landing/` to GitHub Pages |

Neither invokes `cargo`. `cargo check`, `cargo test`, `cargo clippy`, and
`just check-everything` run **only on a developer's laptop, only on macOS
arm64**, and only if that developer remembers. Nothing in the repository proves
the workspace compiles for Windows or Linux until someone runs
`scripts/release.sh backends` — i.e. **at ship time, on the release-cutter's
machine, with the release already half-cut.**

### 2. Even the release does not compile most of the code for Windows/Linux.

Both cross recipes in `scripts/release.sh` build **two binaries only**:

```sh
# scripts/release.sh:174  (linux)
cargo build --release --target x86_64-unknown-linux-gnu --bin biorouterd --bin biorouter
# scripts/release.sh:198  (windows)
cargo build --release --target x86_64-pc-windows-gnu   --bin biorouterd --bin biorouter
```

`--bin X --bin Y` compiles those two binaries and the library crates they pull
in. It does **not** compile:

- `#[cfg(test)]` modules and `tests/` integration targets (never cross-compiled
  by anything, ever — including `crates/biorouter-sandbox/tests/sandbox.rs`,
  which is itself `cfg`-gated);
- `crates/biorouter-bench`, `crates/biorouter-test`, `crates/biorouter-headless`,
  `crates/biorouter-acp` (not in the dependency closure of the two bins);
- examples and benches.

Platform breaks hide in exactly those places, because test code is where people
write `use std::os::unix::fs::PermissionsExt` without a second thought.

### 3. The `cfg` surface is large and growing, and BR-20/BR-37/BR-64 all touched it.

`grep -rl 'cfg(unix)\|cfg(windows)\|cfg(target_os\|cfg(target_family' crates` →
**33 files**, plus four crates with target-conditional *dependency tables*, which
is the most dangerous kind of `cfg` because a mistake produces an unresolved
symbol, not a warning:

```toml
# crates/biorouter-cli/Cargo.toml:68-73
[target.'cfg(not(target_os = "windows"))'.dependencies]
tikv-jemallocator = { workspace = true, optional = true }   # default feature!
[target.'cfg(target_os = "windows")'.dependencies]
winapi = { version = "0.3", features = ["wincred"] }
# crates/biorouter-server/Cargo.toml:50,59 — same jemalloc split + winreg (windows-only)
# crates/biorouter/Cargo.toml:126,131      — winapi (windows) + libc (unix)
```

`jemalloc` is a **default feature** that is silently empty on Windows. Any code
that reaches for `tikv_jemalloc_ctl` outside a `cfg(not(windows))` block builds
green on every developer machine and fails only inside the mingw container.

Verified state of the three recent platform-sensitive changes:

- **BR-64 (Seatbelt)** — `crates/biorouter-sandbox/src/seatbelt.rs:168`:
  `pub fn available() -> bool { cfg!(target_os = "macos") && Path::new(SANDBOX_EXEC).exists() }`.
  Correct, and correctly opt-in via `BIOROUTER_SHELL_SANDBOX`
  (`developer/shell.rs:107-160`). But on Windows/Linux the shell runs
  **unsandboxed with a warning**, and *no CI compiles the Linux side of that
  module*. There is **no Landlock anywhere in the tree** (`grep -ri landlock
  crates` → 0 hits), so BR-64 Slice 2 will land Linux-only kernel code into a
  repo with zero Linux compile coverage. BR-70 must land *first*.
- **BR-37 (process-group kill)** — `crates/biorouter-mcp/src/developer/background.rs:343-360`
  is genuinely dual-armed (`#[cfg(unix)]` `libc::kill(-pid, SIGTERM)` → `SIGKILL`;
  `#[cfg(windows)]` `taskkill /T /F /PID`), and `is_group_leader` has an honest
  Windows stub at `:502-505`. The quality is good. **But it is unproven**: the
  Windows arm has never been type-checked by CI, and `libc = "0.2"` is an
  *unconditional* dependency of `biorouter-mcp` (`Cargo.toml:69`) while
  `libc::kill` exists only on unix — one stray edit outside the `cfg` block and
  the Windows build dies at release time.
- **BR-20 (catastrophic denylist)** — `crates/biorouter/src/security/patterns.rs`.
  `CATASTROPHIC_RULES` (`:463`) is `rm_rf_root`, `mkfs_device`, `dd_device`
  — matched by `is_rm_rf_root` / `is_mkfs_device` (`:517-531`), all POSIX. The
  advisory `THREAT_PATTERNS` list contains exactly **one** Windows-aware entry,
  `powershell_download_exec` (`:101`), and **zero** Windows *destructive*
  patterns: no `Remove-Item -Recurse -Force`, no `del /f /s /q`, no `rd /s`, no
  `format C:`. Meanwhile `developer/shell.rs:34-62` correctly selects PowerShell
  (then `cmd.exe`) on Windows. **On Windows the catastrophic safety net is
  effectively absent.**

> **Scope note.** Adding the Windows destructive rules is a BR-20 follow-up
> (tracked separately as *BR-20-W*), not BR-70. What BR-70 owns is the **venue**:
> a real `windows-latest` runner where a `#[cfg(windows)]` PowerShell-pattern
> test can actually execute, and a compile gate that stops the Windows arm of
> BR-37/BR-64 rotting. Without BR-70 there is nowhere to run the proof.

### 4. The glibc floor is protected by a comment and a release-time smoke test.

`scripts/release.sh:146` pins `LINUX_RUST_IMG="rust:1.92-bullseye"` (glibc 2.31)
with a good comment explaining that rolling `rust:latest` (trixie, glibc 2.39)
produces binaries that will not start on Debian 12 / Ubuntu 22.04 / Rocky 9.
But **the `Justfile` still uses `rust:latest`** for its Linux cross recipe
(`Justfile:125`, `make-ui-linux`) — the recipes have **already forked**. That is
the exact drift this ticket must not add more of. The floor is currently checked
only by the `cli-linux` phase's container smoke test, which runs late, manually,
and only during a release.

---

## Goal

A PR-time gate that fails within minutes when a `cfg` mistake, a target-dep
mistake, or a glibc-floor regression is introduced — reusing **the release's own
recipes**, never a second copy of them.

Non-goals: running the full Electron/Playwright suite on three OSes; producing
release artifacts in CI; replacing the release pipeline.

---

## Design

### D1. One recipe, three callers: `scripts/cross-env.sh`

The single highest-value change is *extraction*, not addition. Today the
Windows/Linux docker incantations exist **twice** (`Justfile:50-108`,
`scripts/release.sh:138-201`) and have already diverged on the one invariant that
matters (`rust:latest` vs `rust:1.92-bullseye`). Adding a third copy for CI would
guarantee the gate tests something the release does not build.

Create `scripts/cross-env.sh` as the **only** place the images, toolchain env,
and linker hacks are written. `release.sh`, the `Justfile`, and CI all source it.

```sh
#!/usr/bin/env bash
# scripts/cross-env.sh — THE single source of truth for cross-compilation.
# Sourced by scripts/release.sh, the Justfile, and .github/workflows/cross.yml.
# Do not inline a docker recipe anywhere else; scripts/check-no-cross-drift.sh
# fails the build if you do.

# Pin the Linux cross-compile to an OLD-glibc base (Debian 11 "bullseye",
# glibc 2.31): binaries must start on Ubuntu 20.04+/22.04, Debian 11/12,
# RHEL/Rocky 8/9. Rolling `rust:latest` (trixie, glibc 2.39) breaks all of them.
# THE GLIBC FLOOR LIVES HERE AND NOWHERE ELSE.
: "${LINUX_RUST_IMG:=rust:1.92-bullseye}"
: "${WIN_RUST_IMG:=rust:latest}"   # mingw: no glibc concern
: "${GLIBC_FLOOR:=2.31}"

# aws-lc-sys (rustls/AWS SDK) compiles a POSIX threading shim under mingw and
# references winpthread symbols; rustc places `-C link-arg=-l…` BEFORE the rlibs,
# so GNU ld discards the lib before it is needed. Wrap the linker to append
# `-lpthread -lwinpthread` AFTER everything else.
WIN_LINKER_WRAP='printf "#!/bin/sh\nexec x86_64-w64-mingw32-gcc \"\$@\" -lpthread -lwinpthread\n" > /usr/local/bin/winpthread-gcc && chmod +x /usr/local/bin/winpthread-gcc'

# lzma-sys (via xz2, the knowledge .brkb path) would otherwise find the HOST
# liblzma through pkg-config and emit an invalid dynamic link. LZMA_API_STATIC=1
# forces it to statically compile its bundled liblzma C source.
#   NOTE: this matters for `cargo check` too — check RUNS build scripts.

ROOT="${ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"

# Run an arbitrary cargo command inside the pinned LINUX cross image.
#   cross_linux "cargo check --workspace --all-targets --locked" [target-dir-suffix]
cross_linux() {
  local cargo_cmd="$1" tdir="${2:-}"
  docker volume create biorouter-linux-bullseye-cache >/dev/null 2>&1 || true
  docker run --rm \
    -v "$ROOT":/usr/src/myapp \
    -v biorouter-linux-bullseye-cache:/usr/local/cargo/registry \
    ${CROSS_TARGET_MOUNT:+-v "$CROSS_TARGET_MOUNT":/cross-target} \
    -w /usr/src/myapp "$LINUX_RUST_IMG" sh -c "
      set -e
      rustup target add x86_64-unknown-linux-gnu
      dpkg --add-architecture amd64 && apt-get update -q
      apt-get install -y --no-install-recommends gcc-x86-64-linux-gnu g++-x86-64-linux-gnu \
        protobuf-compiler cmake libxcb1-dev:amd64 libbz2-dev:amd64
      export CC_x86_64_unknown_linux_gnu=x86_64-linux-gnu-gcc \
             CXX_x86_64_unknown_linux_gnu=x86_64-linux-gnu-g++ \
             AR_x86_64_unknown_linux_gnu=x86_64-linux-gnu-ar \
             CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc \
             LZMA_API_STATIC=1 PKG_CONFIG_ALLOW_CROSS=1 \
             PKG_CONFIG_PATH_x86_64_unknown_linux_gnu=/usr/lib/x86_64-linux-gnu/pkgconfig \
             PROTOC=/usr/bin/protoc
      ${tdir:+export CARGO_TARGET_DIR=$tdir}
      $cargo_cmd --target x86_64-unknown-linux-gnu"
}

# Run an arbitrary cargo command inside the WINDOWS (mingw) cross image.
cross_windows() {
  local cargo_cmd="$1" tdir="${2:-}"
  docker volume create biorouter-windows-cache >/dev/null 2>&1 || true
  docker run --rm \
    -v "$ROOT":/usr/src/myapp \
    -v biorouter-windows-cache:/usr/local/cargo/registry \
    ${CROSS_TARGET_MOUNT:+-v "$CROSS_TARGET_MOUNT":/cross-target} \
    -w /usr/src/myapp "$WIN_RUST_IMG" sh -c "
      set -e
      rustup target add x86_64-pc-windows-gnu
      apt-get update && apt-get install -y mingw-w64 protobuf-compiler cmake
      $WIN_LINKER_WRAP
      export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc \
             CXX_x86_64_pc_windows_gnu=x86_64-w64-mingw32-g++ \
             AR_x86_64_pc_windows_gnu=x86_64-w64-mingw32-ar \
             CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=/usr/local/bin/winpthread-gcc \
             LZMA_API_STATIC=1 PKG_CONFIG_ALLOW_CROSS=1 PROTOC=/usr/bin/protoc \
             PATH=/usr/bin:\$PATH
      ${tdir:+export CARGO_TARGET_DIR=$tdir}
      $cargo_cmd --target x86_64-pc-windows-gnu"
}
```

`release.sh` then becomes (behaviour-identical, recipe deleted):

```sh
. "$ROOT/scripts/cross-env.sh"
cmd_linux-backend() {
  ensure_docker
  log "cross-compiling linux-gnu backend (docker, $LINUX_RUST_IMG)"
  rm -rf "$ROOT/target/x86_64-unknown-linux-gnu/release/biorouter"{,d}
  purge_elf_build_scripts            # existing workaround, unchanged
  cross_linux "cargo build --release --bin biorouterd --bin biorouter"
}
```

and `Justfile`'s `make-ui-linux` / `release-windows` call `cross_linux` /
`cross_windows` too — which, as a side effect, **fixes the already-live
`rust:latest` glibc bug in `Justfile:125`.**

**Anti-drift guard** (`scripts/check-no-cross-drift.sh`, wired into
`just check-everything` and CI):

```sh
#!/usr/bin/env bash
# Fail if a docker cross-compile recipe is written anywhere but scripts/cross-env.sh.
set -euo pipefail
bad=$(grep -rn --exclude=scripts/cross-env.sh --exclude-dir=.git --exclude-dir=docs \
        -E 'x86_64-(pc-windows-gnu|unknown-linux-gnu)' Justfile scripts/ .github/ \
      | grep -vE 'cross-env\.sh|cross_linux|cross_windows|check-no-cross-drift' || true)
[ -z "$bad" ] || { echo "::error::cross-compile recipe outside scripts/cross-env.sh:"; echo "$bad"; exit 1; }
# The glibc floor must never be raised by switching to a rolling image.
grep -q 'LINUX_RUST_IMG:=rust:1.92-bullseye' scripts/cross-env.sh \
  || { echo "::error::LINUX_RUST_IMG drifted off the bullseye (glibc 2.31) pin"; exit 1; }
echo "OK — one cross recipe, floor intact."
```

### D2. What to check: `--workspace --all-targets --locked`

```
cargo check --workspace --all-targets --locked
```

per target, for `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-gnu`.

- **`--workspace`** — all 9 crates, including `biorouter-bench`,
  `biorouter-test`, `biorouter-headless`, `biorouter-acp`, which the release's
  `--bin biorouterd --bin biorouter` never touches.
- **`--all-targets`** — lib + bins + **tests** + benches + examples. This is the
  point of the ticket: `#[cfg(test)]` code is where platform breaks hide, and it
  is the *only* code in the tree that is currently cross-compiled by nothing.
  `crates/biorouter-sandbox/tests/sandbox.rs` and
  `crates/biorouter-mcp/src/developer/analyze/tests/traversal_tests.rs` are
  already `cfg`-gated and already unverified.
- **`--locked`** — a cross job must never silently update `Cargo.lock`.
- **Default features.** Do *not* use `--all-features`: `jemalloc` is a default
  feature that is target-conditionally empty on Windows, and enabling every
  feature would test a configuration nobody ships. Default features == shipped
  configuration. (A `--no-default-features` lane is optional and low value here.)
- **If a crate genuinely cannot compile for a target, gate it in `Cargo.toml`**
  (`[target.'cfg(...)'.dependencies]`, or `#[cfg]` at the module root) — do
  **not** `--exclude` it from CI. An exclusion is an unverified assertion; a
  `cfg` is a checked one.

**What `cargo check` does and does not catch — say it out loud.**

| Caught by `cargo check` | Not caught |
|---|---|
| `cfg` mistakes, missing `#[cfg(windows)]` arms | **Link** errors (the aws-lc / winpthread class) |
| Target-dep / feature mistakes (jemalloc, winapi, winreg) | glibc symbol-version regressions |
| Type errors in `#[cfg(test)]` code | Anything runtime |
| **Build-script failures** — `check` *does* run `build.rs`, so `lzma-sys`, `aws-lc-sys` (cmake), and `protoc` still execute and still need `LZMA_API_STATIC=1`, `PROTOC`, `cmake`, mingw. This is why the check must use the real recipe, not a bare `cargo check --target`. | |

Consequence: **check is the PR gate; the full cross *build* is a scheduled
job.** A nightly `cross-build` workflow runs `cargo build --release --bin
biorouterd --bin biorouter` for both targets — exactly the release command — so a
link regression is caught within 24 h rather than at ship time. Same
`cross-env.sh`, one extra `on: schedule` job. This is a deliberate
speed/coverage split, not an oversight: linking the whole workspace on every PR
would cost ~20+ min per target.

### D3. Caching

Three layers, mirroring what the release already does:

1. **Cargo registry** — the docker named volumes the release already uses
   (`biorouter-linux-bullseye-cache`, `biorouter-windows-cache`). On a GitHub
   runner, named volumes do not survive the job, so `cross-env.sh` honours a
   `CROSS_REGISTRY_MOUNT` override that binds a host path instead, which
   `actions/cache` then persists (key: `Cargo.lock` hash).
2. **Per-triple target dir** — `CARGO_TARGET_DIR=/cross-target/<triple>`, backed
   by `CROSS_TARGET_MOUNT`. Two reasons:
   - *Speed*: incremental `check` on a warm target dir is ~2-4 min vs ~12-18 min
     cold (aws-lc-sys + tiktoken + protobuf dominate).
   - *Correctness*: it **structurally eliminates the ELF-build-script clobber**
     that `release.sh:157-162` currently works around with a
     `find … -name build-script-build … | grep ELF && rm -rf` hack. That hack
     exists because the mac and linux builds share `target/release/build/`. A CI
     check with its own target dir per triple can never hit it. (Leave the hack
     in `release.sh` — it still shares a target dir with the mac build.)
3. **Image layer cache** — `apt-get install mingw-w64 protobuf-compiler cmake`
   inside the container on every run costs ~60-90 s. Slice 3 (optional) bakes
   two small `Dockerfile`s (`ci/cross/linux.Dockerfile`,
   `ci/cross/windows.Dockerfile`) built `FROM $LINUX_RUST_IMG` / `$WIN_RUST_IMG`
   and pushed to GHCR, so the toolchain install is a cached layer. Keep the pin
   in `cross-env.sh` as the `FROM` argument so there is still one floor.

Budget: cold ≈ 15 min/target (parallel jobs ⇒ 15 min wall), warm ≈ 3-5 min.

### D4. Where the platform-specific *unit tests* run

A docker cross-**check** cannot run a test — it does not link, and it targets a
foreign OS. Behavioural coverage needs real kernels. Hence a second, orthogonal
axis: a native `cargo test` matrix.

| Job | Runner | Target | Proves |
|---|---|---|---|
| `test (macos-latest)` | macOS arm64 | `aarch64-apple-darwin` | Seatbelt (BR-64) actually spawns `sandbox-exec`; `available()` is `true`; unix process-group kill (BR-37) |
| `test (ubuntu-latest)` | Linux x86_64 | `x86_64-unknown-linux-gnu` | Landlock (BR-64 Slice 2, once it lands — **this runner is its only venue**); unix denylist; `libc::kill(-pid)` |
| `test (windows-latest)` | Windows | `x86_64-pc-windows-**msvc**` | `detect_windows_shell()` really finds `pwsh`/`powershell`/`cmd.exe` (`shell.rs:34-62`); `taskkill /T` really reaps a job tree (`background.rs:353-360`); the **Windows destructive-pattern tests of BR-20-W** |

**The msvc/gnu split is a real subtlety and must be stated.** We *ship*
`x86_64-pc-windows-gnu` (mingw cross). GitHub's `windows-latest` natively
defaults to `x86_64-pc-windows-msvc`. These are different ABIs and different
libc. Therefore both axes are load-bearing and neither substitutes for the other:

- the **docker gnu check** proves *the thing we ship* compiles;
- the **windows-latest msvc test** proves *the Windows logic behaves*, on a real
  Windows kernel with a real PowerShell and a real `taskkill`.

Optionally add `rustup target add x86_64-pc-windows-gnu` + a `cargo check
--target x86_64-pc-windows-gnu` step on `windows-latest` as a cheap third
data point; it is not a substitute for the docker job, which is what the release
actually uses.

Native tests run `cargo test --workspace --all-targets` but must **exclude the
network/cassette-dependent suites** on the matrix (they need `BIOROUTER_RECORD_MCP`
fixtures and provider keys). Start with `--lib --bins` on all three OSes plus the
explicitly-safe integration tests, and widen once green — a red-on-arrival matrix
gets disabled, which is worse than no matrix.

### D5. The glibc floor invariant, and how CI protects it

The floor is `glibc 2.31` (Debian 11 bullseye). A `cargo check` **cannot** see
it — checks do not link, so no symbol versions are emitted. Three defences,
cheap → expensive:

1. **Pin assertion (every PR, ~0 s)** — `check-no-cross-drift.sh` (D1) greps
   `scripts/cross-env.sh` for the literal `rust:1.92-bullseye` and fails if the
   image drifted. This also finally kills the *existing live bug* at
   `Justfile:125`, which still says `rust:latest`.
2. **Symbol-floor assertion (nightly + release, ~1 min after the build)** —
   `scripts/check-glibc-floor.sh`, run against the freshly cross-built binaries:

   ```sh
   #!/usr/bin/env bash
   # Assert no binary requires a glibc newer than the floor (default 2.31).
   set -euo pipefail
   . "$(dirname "$0")/cross-env.sh"
   BIN_DIR="${1:-target/x86_64-unknown-linux-gnu/release}"
   worst=$(docker run --rm -v "$PWD":/w -w /w "$LINUX_RUST_IMG" sh -c "
       objdump -T $BIN_DIR/biorouterd $BIN_DIR/biorouter 2>/dev/null \
         | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -V | tail -1")
   if [ "$(printf '%s\n%s\n' "$GLIBC_FLOOR" "$worst" | sort -V | tail -1)" != "$GLIBC_FLOOR" ]; then
     echo "::error::binaries require glibc $worst > floor $GLIBC_FLOOR — Debian 12 / Ubuntu 22.04 / Rocky 9 would fail to start"
     exit 1
   fi
   echo "OK — max glibc requirement $worst ≤ floor $GLIBC_FLOOR"
   ```

   This is the check that would have *mechanically* caught the trixie regression
   the `release.sh:140-145` comment describes, instead of a human noticing a
   container smoke test failed.
3. **Runtime smoke (release only, unchanged)** — the existing `cli-linux` phase
   already boots the binaries in clean Debian/Rocky containers. Keep it as the
   backstop; it is the only defence that proves the *dynamic loader* is happy.

---

## The Justfile recipe

```make
# Cross-platform compile gate. Same docker images, same flags, same linker hacks
# as `scripts/release.sh` — both source scripts/cross-env.sh, so this can never
# drift from what the release actually builds.
#
# `cargo check` (not build): it type-checks every crate, every target — INCLUDING
# #[cfg(test)] code, which no release build ever compiles for Windows/Linux —
# without paying for a link. It still runs build.rs, so lzma-sys / aws-lc-sys /
# protoc are all exercised. Link errors are covered by the nightly cross-build.
check-cross: check-cross-linux check-cross-windows

check-cross-linux:
    #!/usr/bin/env bash
    set -euo pipefail
    . scripts/cross-env.sh
    echo "→ cargo check x86_64-unknown-linux-gnu ($LINUX_RUST_IMG, glibc floor $GLIBC_FLOOR)"
    cross_linux "cargo check --workspace --all-targets --locked" \
                "/usr/src/myapp/target/cross-check/linux"

check-cross-windows:
    #!/usr/bin/env bash
    set -euo pipefail
    . scripts/cross-env.sh
    echo "→ cargo check x86_64-pc-windows-gnu ($WIN_RUST_IMG)"
    cross_windows "cargo check --workspace --all-targets --locked" \
                  "/usr/src/myapp/target/cross-check/windows"

# Full cross BUILD of the shipped binaries — catches LINK errors (the aws-lc /
# winpthread class) that `check` cannot. Nightly in CI; run locally before a release.
build-cross:
    #!/usr/bin/env bash
    set -euo pipefail
    . scripts/cross-env.sh
    cross_linux   "cargo build --release --bin biorouterd --bin biorouter"
    cross_windows "cargo build --release --bin biorouterd --bin biorouter"
    ./scripts/check-glibc-floor.sh
```

and `check-everything` gains one line:

```make
check-everything:
    ...
    @echo "  → Checking cross-compile recipes have not drifted..."
    ./scripts/check-no-cross-drift.sh
```

(`check-cross` is deliberately **not** added to `check-everything` — it needs
Docker and takes minutes. It is a CI gate and an on-demand local command.)

---

## The CI workflow

`.github/workflows/rust.yml` — new file. Note this is also the repo's **first
`cargo` CI of any kind**, so it includes the baseline host lanes too.

```yaml
name: Rust

on:
  pull_request:
  push:
    branches: [main]
  schedule:
    - cron: "0 7 * * *"   # nightly: the expensive full cross-BUILD + glibc floor

concurrency:
  group: rust-${{ github.ref }}
  cancel-in-progress: true

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: -D warnings

jobs:
  # ── 1. Cheap guards ────────────────────────────────────────────────────────
  guards:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Cross recipes have not drifted / glibc floor pin intact
        run: ./scripts/check-no-cross-drift.sh
      - name: Version consistency (CLI = daemon = GUI)
        run: ./scripts/check-version-consistency.sh

  # ── 2. Cross COMPILE gate — the BR-70 core. Docker, same images as release.sh.
  cross-check:
    if: github.event_name != 'schedule'
    runs-on: ubuntu-latest          # docker host; the TARGET is what matters
    strategy:
      fail-fast: false
      matrix:
        target: [x86_64-unknown-linux-gnu, x86_64-pc-windows-gnu]
    steps:
      - uses: actions/checkout@v4

      # Registry + per-triple target dir, persisted across runs. The per-triple
      # target dir is why this is minutes, not a quarter-hour — and it also makes
      # the mac/linux build-script clobber (release.sh:157) structurally impossible.
      - name: Restore cross cache
        uses: actions/cache@v4
        with:
          path: |
            .cross-cache/registry
            .cross-cache/target/${{ matrix.target }}
          key: cross-${{ matrix.target }}-${{ hashFiles('Cargo.lock', 'rust-toolchain.toml', 'scripts/cross-env.sh') }}
          restore-keys: cross-${{ matrix.target }}-

      - name: cargo check --workspace --all-targets
        run: |
          set -euo pipefail
          mkdir -p .cross-cache/registry .cross-cache/target/${{ matrix.target }}
          export CROSS_REGISTRY_MOUNT="$PWD/.cross-cache/registry"
          export CROSS_TARGET_MOUNT="$PWD/.cross-cache/target/${{ matrix.target }}"
          . scripts/cross-env.sh
          case "${{ matrix.target }}" in
            x86_64-unknown-linux-gnu)
              cross_linux  "cargo check --workspace --all-targets --locked" /cross-target ;;
            x86_64-pc-windows-gnu)
              cross_windows "cargo check --workspace --all-targets --locked" /cross-target ;;
          esac

  # ── 3. Native unit tests on REAL kernels. This is where platform BEHAVIOUR is
  #      proven: PowerShell selection + taskkill (windows), Seatbelt (macos),
  #      Landlock (ubuntu, once BR-64 slice 2 lands).
  #      NOTE: windows-latest is MSVC; we SHIP mingw/gnu. Job 2 covers the gnu
  #      ABI, this job covers the Windows behaviour. Both are required.
  test:
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master
        with: { toolchain: "1.92", components: clippy }
      - uses: Swatinem/rust-cache@v2
        with: { key: ${{ matrix.os }} }

      - name: Install protoc + cmake (build-script deps)
        uses: arduino/setup-protoc@v3
        with: { repo-token: "${{ secrets.GITHUB_TOKEN }}" }

      - name: cargo test (workspace, lib + bins)
        run: cargo test --workspace --lib --bins --locked

      # Cross-platform-sensitive integration suites, run everywhere they apply.
      - name: cargo test (sandbox + developer shell/background)
        run: |
          cargo test -p biorouter-sandbox --locked
          cargo test -p biorouter-mcp --lib developer:: --locked
          cargo test -p biorouter    --lib security::  --locked

      - name: clippy (host)
        if: matrix.os == 'ubuntu-latest'
        run: cargo clippy --workspace --all-targets --locked

  # ── 4. Nightly: the FULL cross BUILD. `check` cannot catch link errors
  #      (aws-lc-sys / winpthread) or glibc symbol-version regressions.
  cross-build-nightly:
    if: github.event_name == 'schedule'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Build shipped binaries for both cross targets (release command)
        run: |
          . scripts/cross-env.sh
          cross_linux   "cargo build --release --bin biorouterd --bin biorouter"
          cross_windows "cargo build --release --bin biorouterd --bin biorouter"
      - name: Assert glibc floor (${{ '2.31' }} / bullseye) is not raised
        run: ./scripts/check-glibc-floor.sh
      - name: Boot the Linux binaries on the oldest supported distro
        run: |
          docker run --rm -v "$PWD":/w debian:bullseye \
            /w/target/x86_64-unknown-linux-gnu/release/biorouter --version
```

---

## Implementation slices

| Slice | Work | Value |
|---|---|---|
| **1** | `scripts/cross-env.sh`; rewrite `release.sh` + `Justfile` to source it (fixes the live `rust:latest` bug in `Justfile:125`); `check-no-cross-drift.sh` into `check-everything`. **No new CI yet.** | Kills the fork before it triples. Standalone win. |
| **2** | `just check-cross` + the `cross-check` CI job. | **The gate.** cfg/target-dep breakage fails at PR time. |
| **3** | The `test` matrix (3 OSes). Land it **before** BR-64 Slice 2 (Landlock) and BR-20-W (Windows denylist) — those have no venue without it. | Behavioural coverage on real kernels. |
| **4** | `check-glibc-floor.sh` + nightly `cross-build`. | Link errors + glibc floor, mechanically. |
| **5** | *(optional)* Pre-baked GHCR cross images. | ~90 s/job. |

Expected first-run outcome: **slice 2 will be red.** ~33 `cfg` files and four
target-dep tables have never been compile-checked for Windows with
`--all-targets`. Budget a fix-forward pass; that backlog *is* the bug this ticket
exists to expose.

## Risks

- **Docker-in-Actions cost.** `ubuntu-latest` has Docker preinstalled; the cost
  is time, not setup. Mitigated by the per-triple target cache; if it is still
  slow, slice 5.
- **Flaky native tests on Windows/macOS runners** (path separators, temp dirs,
  no `sandbox-exec` on a GH mac runner? — `available()` degrades to `false`,
  which the tests must tolerate). Start narrow (`--lib --bins`), widen when green.
- **`RUSTFLAGS: -D warnings` on a first-ever CI** may fail immediately on
  existing warnings. Land it as a separate follow-up commit if so; do not let it
  block the cross gate.

## Test plan

1. `just check-cross` green on a clean tree.
2. Introduce a deliberate break — e.g. call `libc::kill` in `background.rs`
   *outside* the `#[cfg(unix)]` block, or use `tikv_jemalloc_ctl` unconditionally
   — and confirm `check-cross-windows` fails while the host build stays green.
3. Add a `#[cfg(test)]`-only unix-ism to a test module; confirm `--all-targets`
   catches it and that a `--bin`-only build (the release's command) does **not**.
   This is the regression that proves `--all-targets` is load-bearing.
4. Flip `LINUX_RUST_IMG` to `rust:latest`; confirm `check-no-cross-drift.sh` fails
   at PR time and `check-glibc-floor.sh` fails nightly.
5. Confirm `release.sh backends` still produces byte-comparable binaries after
   the `cross-env.sh` extraction (same flags ⇒ same output).
