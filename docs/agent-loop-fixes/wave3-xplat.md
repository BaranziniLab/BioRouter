# Wave 3 — `xplat` cluster verification report

Worktree: `/Users/wanjun/Desktop/BioRouter/.worktrees/xplat`
Branch: `agent-loop-xplat`
Base: `agent-loop-integration` (merge-base `a54c4d79`)
Verifier date: 2026-07-13
Isolated target dir: `/Users/wanjun/.cache/br-targets/xplat` (`CARGO_INCREMENTAL=0`)

## Verdict

**GREEN.** Full workspace regression: **2398 passed, 1 failed** across 60 test-result
lines. The single failure is `test_anthropic_provider` (KNOWN-ALLOWED — live Anthropic
API). Zero new failures. fmt clean, clippy `-D warnings` clean after one regression fix,
no new `too_many_lines` baseline violations.

## Proposals in this cluster

| Commit | ID | Summary | Key files | Tests |
|--------|----|---------|-----------|-------|
| `3d6d3aa9` | GAP-2 | PID-reuse guard for the Windows orphan reaper + graceful Windows kill phase (`same_process` fingerprint before any force-kill; memoized liveness probes) | `crates/biorouter-mcp/src/developer/background.rs` | `biorouter_mcp` lib (636 passed) |
| `651acff0` | BR-68 | Cross-platform command safety: platform+dialect-aware tokenizers (POSIX/PowerShell/cmd.exe), Windows destructive-command floor, Linux/macOS policy parity | `crates/biorouter/src/security/policy/{target,pwsh,cmd_shell,command,rule,mod,baseline}.rs` + `tests_platform.rs` + 4 `baseline.*.policy.yaml` | `biorouter` lib (1189 passed) incl. `tests_platform` |
| `2d16ff0a` | BR-69 | Cross-platform shell sandbox behind one `ShellSandbox` trait (macOS Seatbelt unchanged; Linux Landlock+seccomp via re-exec helper; honest Windows tier:None); `biorouter doctor` Shell-sandbox section | new crate `crates/biorouter-sandbox/src/shell_sandbox/{mod,macos,linux,windows}.rs`; `developer/shell.rs`; `cli/…/doctor.rs`; `biorouter`/`biorouterd` `main.rs` helper wiring | `biorouter_sandbox` lib (17 passed) + `sandbox.rs` (16 passed) |
| `ab721780` | BR-70 | Cross-platform CI verification gate: one `check-cross` recipe, `cross-env.sh`, glibc-floor + no-cross-drift guards, Rust CI workflow | `.github/workflows/rust.yml`, `Justfile`, `scripts/{cross-env,check-glibc-floor,check-no-cross-drift,build-headless-linux,release}.sh` | scripts/CI only (no unit tests) |

`GAP-2` uses the campaign's `GAP-N` naming (BR-68's own message references `GAP-1/GAP-3`),
carries a full descriptive body, and is a coherent standalone commit — not orphaned junk.
Working tree was clean at intake; no orphaned work to reconcile.

## Regression fixed by the verifier

| Commit | ID | What |
|--------|----|------|
| `ba2100b0` | BR-68 (fix) | Clippy `-D warnings` failed with **13 errors** introduced by BR-68's tokenizers: 10 `clippy::string_slice` (workspace `string_slice = "warn"`) in `target.rs`/`pwsh.rs`, plus 3 dead-code items (`blast_of`, `PwshSegment::raw_binary`, `PwshSegment::has_param`) that are used only in `#[cfg(test)]` code and so read as dead in the non-test lib build clippy `--all-targets` compiles. |

Fix: every flagged slice sits behind an ASCII drive-letter / quote / `/` guard, so the byte
bounds are proven char boundaries; annotated the six enclosing functions
(`depth`, `split_drive`, `strip_quotes`, `split_windows_root`,
`split_windows_root_nocwd`, `is_obfuscated_exec`) with `#[allow(clippy::string_slice)]`
+ justification, and the three test-only items with `#[allow(dead_code)]` + justification.
This is the house style (matches the existing `#[allow(clippy::too_many_arguments)]`
annotations in `command.rs`/`mod.rs`). No behavior change. Re-ran clippy: `-D warnings`
passes, build-finished `success:true`.

## Gate evidence

### 1. Disk
`df -h /` → 27 GiB avail (> 8 GiB floor). OK.

### 2. Commits / working tree
5 commits over base (4 proposals + 1 verifier fix). `git status --porcelain` clean at
intake. No orphaned work.

### 3. `cargo fmt --all -- --check`
Exit 0, clean. No `style: cargo fmt` commit needed.

### 4. Clippy — `./scripts/clippy-lint.sh`
- `cargo clippy --all-targets -- -D warnings`: **passes** after `ba2100b0` (was 13 errors).
- Baseline `too_many_lines`: the script's jq parser hit its **known bug**
  (`jq: error (at <stdin>:1126): split input and separator must be strings`) and printed
  a bogus `ok`, so its exit was **not trusted**. Cross-checked by hand: extracted all 13
  live `too_many_lines` sites from the clippy JSON and mapped each to
  `clippy-baselines/too_many_lines.txt`. All 12 sites in touched/covered crates are
  already baselined (incl. `doctor.rs::handle_doctor`, which BR-69 grew but which was
  pre-baselined). The one un-baselined site,
  `biorouter-bench/…/simple_repo_clone_test.rs:22`, is in a crate this cluster never
  touched and exists unchanged at the merge-base — a pre-existing violation the jq bug
  has always silently dropped, **not** a cluster regression. No new violation.

### 5. OpenAPI
Skipped — no `biorouter-server/src/routes/` change (only `server/src/main.rs` sandbox
helper wiring).

### 6. Compile all targets
`cargo test --workspace --no-run` — all executables built, no `E0063`/`E0004`/errors.

### 7. Full regression — `cargo test --workspace --no-fail-fast`
2398 passed / 1 failed / 60 result-lines. Log:
`/Users/wanjun/.cache/br-baseline/xplat-wave3-test.log`.
Only failure: `test_anthropic_provider` (KNOWN-ALLOWED, live Anthropic API,
`providers.rs:251`). `tunnel::lapstone_test` did not fail. Exceeds gate baseline
(2332 passed / 59 suites) because BR-69 added the `biorouter-sandbox` crate
(17 lib + 16 integration tests) and BR-68 added `tests_platform`.

### 8. ui/desktop
Skipped — no `ui/desktop` change in the range.

## Per-suite evidence (selected)

```
biorouter          lib      1189 passed; 0 failed  (incl. security::policy::* + tests_platform)
biorouter-mcp      lib       636 passed; 0 failed; 2 ignored  (incl. GAP-2 background reaper)
biorouter-cli      lib       173 passed; 0 failed
biorouter-sandbox  lib        17 passed; 0 failed          (BR-69, new crate)
biorouter-sandbox  sandbox    16 passed; 0 failed; 1 ignored
biorouter-server   lib        65 passed; 0 failed
biorouterd         bin        64 passed; 0 failed
biorouter-acp      lib        16 passed; 0 failed
knowledge_routes            31 passed; 0 failed
llamacpp_routes              6 passed; 0 failed
providers          test      14 passed; 1 FAILED  (test_anthropic_provider — allowed)
```

## Must-knows

- BR-68 shipped with 13 clippy errors — the proposal author did not run
  `./scripts/clippy-lint.sh`. Fixed in `ba2100b0`. Future authors touching the policy
  tokenizers: the workspace denies `clippy::string_slice`; guard slices or annotate.
- The `clippy-lint.sh` jq baseline parser is still broken (aborts at the first span with
  no `fn `). Its `too_many_lines: ok` is meaningless; always hand-cross-check.
- `biorouter-bench/…/simple_repo_clone_test.rs::…` (168/100 lines) is a real, pre-existing
  `too_many_lines` violation missing from the baseline file — harmless but worth adding to
  `clippy-baselines/too_many_lines.txt` on a future pass so the true count is honest.
- BR-69's Linux/Windows sandbox arms cannot be compiled on this macOS host (no mingw /
  Linux target); they were type-checked by flipping cfg gates per the commit message. The
  pure cross-platform guard logic is unit-tested on macOS (`biorouter-sandbox` lib, 17).
