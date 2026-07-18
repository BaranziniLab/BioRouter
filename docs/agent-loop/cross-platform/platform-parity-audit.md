# Platform parity audit — agent-loop fix campaign

> **What this is.** A per-file Windows/Linux/macOS audit of the agent-loop fix campaign's
> diff, classifying each feature OK/GAP/BREAK, ranking eight GAP findings by user impact,
> and specifying the commands that would verify each platform. It is the document that
> coined the **GAP-N** identifiers used across the campaign.
> **Status:** Historical record — the audit's top four findings were remediated in Wave 3
> (see [Finding status after Wave 3](#finding-status-after-wave-3)); GAP-6, GAP-7 and GAP-8
> appear to remain open, so the tail still has reference value.
> **Audience:** developers working on cross-platform behaviour of the agent loop's
> security, process and checkpoint subsystems.

Two identifier schemes appear below. **BR-NN** is a proposal from the agent-loop review's
master list — BR-1, BR-2, BR-17, BR-20, BR-21, BR-23, BR-37, BR-42, BR-43, BR-44, BR-64
and BR-65 are all cited here; each is defined in
[improvement proposals](../../history/agent-loop-review/improvement-proposals.md).
**GAP-N** is this document's own coinage for a per-platform degradation, numbered in the
[GAP findings](#gap-findings--ranked-by-user-impact) section below; other campaign
documents cite these numbers without redefining them, so this file is the canonical key.

> **Reproducibility caveats.** The audit records no date and no "performed against commit
> X" anchor, and its scope line names `main...agent-loop-integration` — a branch that no
> longer exists — so the diff it examined cannot be reconstructed exactly. Every `file:line`
> citation below (`patterns.rs:463-537`, `background.rs:502-506`, and the rest) was accurate
> against that tree and has since drifted with the shipped fixes. Read the citations as
> pointers to the right function, not to the right line.

**Scope:** every file in `git diff --name-only main...agent-loop-integration -- crates` (96 files).
**Question:** BioRouter ships macOS (dmg), Windows (zip), Linux (deb/rpm). Does the campaign hold up on all three?
**Verdict:** **no BREAK findings — the branch compiles and runs on all three platforms.** But the campaign's headline *safety* work (BR-20 catastrophic denylist, BR-21 policy engine, BR-64 sandbox) is **POSIX-only in substance**. On Windows a user gets the *appearance* of a safety net (the engine runs, rules load, self-tests pass) with **effectively zero coverage of Windows-destructive commands**. That is the single most important finding in this document.

Classification key: **OK** = portable / correctly `cfg`-gated · **GAP** = compiles, but absent or degraded on a platform · **BREAK** = would fail to compile or panic.

## Finding status after Wave 3

The audit escalated eight findings. Wave 3's cross-platform cluster addressed the top
four; the evidence is in the
[cross-platform cluster verification report](parity-verification-report.md).

| Finding | Status | Remediated by |
|---|---|---|
| GAP-1 — Windows catastrophic safety net absent | Resolved | BR-68 (`baseline.windows/linux/macos.policy.yaml` now exist) — see [command safety](command-safety.md) |
| GAP-2 — Windows orphan reaper has no PID-reuse guard | Resolved | The PID-reuse fix, commit `3d6d3aa9` |
| GAP-3 — POSIX tokenizer mangles Windows paths | Resolved | BR-68 (`security/policy/{pwsh,cmd_shell}.rs` tokenizers) |
| GAP-4 — sandbox exists on one of three platforms | Resolved | BR-69 (`crates/biorouter-sandbox/src/shell_sandbox/{linux,windows}.rs`) — see [Linux and Windows sandboxing](linux-and-windows-sandboxing.md) |
| GAP-5 — no graceful-shutdown window on Windows | Partly resolved | The graceful Windows kill phase shipped with commit `3d6d3aa9`; the Job Object is not recorded as shipped |
| GAP-6 — `undo_edit` misses PowerShell writes | Appears open | — |
| GAP-7 — Windows managed-policy trust check is a no-op | Appears open | — |
| GAP-8 — checkpoint exec bits and locked files on Windows | Appears open | — |
| "What CI should run on every PR" | Resolved | BR-70 (`.github/workflows/rust.yml`, `scripts/cross-env.sh`) — see [the CI gate](ci-gate.md) |

Read the sections below as the state of the tree *at audit time*. Where a finding is
marked resolved above, treat its recommended fix as a record of what was done rather than
as work outstanding.

## Compatibility matrix

The `Status` column carries the severity; the per-platform columns say what each platform
actually gets.

| Feature / BR | macOS | Windows | Linux | Status | Notes |
|---|---|---|---|---|---|
| **BR-64** OS shell sandbox (`crates/biorouter-sandbox/src/seatbelt.rs`) | Seatbelt, kernel-enforced | none | none | **GAP** | `available()` = `cfg!(target_os="macos") && /usr/bin/sandbox-exec` exists (`seatbelt.rs:169`). Off-platform → `warn!` + run **unsandboxed** (`shell.rs:135-141`). Opt-in (`BIOROUTER_SHELL_SANDBOX`), so no regression, but the feature is 1-of-3 platforms. |
| **BR-20** catastrophic denylist (`security/patterns.rs:463-537`) | full | degraded: ~1 of 8 rules can fire | full | **GAP (highest impact)** | All 8 rules are Unix-shaped. Zero Windows/PowerShell destructive commands. See GAP-1. |
| **BR-21** policy engine (`security/policy/`) | full | degraded: rules load, ~never match | full | **GAP** | All 10 baseline rules key on POSIX binaries + `/`-rooted `path_glob` (`baseline.policy.yaml:26-231`). Only `baseline.system_power_off` (`binary: [shutdown]`) can plausibly fire on Windows. |
| **BR-21** argv tokenizer (`security/policy/command.rs`) | full | degraded: backslash-mangling | full | **GAP** | `shlex::split` (`command.rs:195`) is a POSIX tokenizer: it treats `\` as an escape, so `C:\Users\x` tokenizes to `C:Usersx`. Path canonicalization is wrong for every Windows path. |
| **BR-37** orphan reaping (`developer/background.rs:363-522`) | full | degraded: no PID-reuse guard | full | **GAP (safety)** | `is_group_leader()` returns a hardcoded `true` on Windows (`background.rs:502-506`) → reap is guarded by liveness alone → a recycled PID can be `taskkill /F /T`'d. See GAP-2. |
| **BR-42** background jobs + process-group kill (`background.rs:343-359`) | SIGTERM→SIGKILL | degraded: `taskkill /F /T`, no graceful phase | full | **GAP (minor)** | Dual-armed and correct in shape. Windows arm force-kills immediately; Unix gives 1.5 s to flush. Also: no Job Object / `CREATE_NEW_PROCESS_GROUP`, so `/T` walks the PPID tree and misses re-parented grandchildren. |
| **BR-42** foreground kill (`developer/shell.rs:210-258`) | portable | portable | portable | **OK** | Windows arm `await`s `taskkill /F /T` **before** `child.kill()` (`shell.rs:249-256`) — correct ordering (kill the tree while the parent still owns it). |
| **BR-43** shadow-git checkpoints (`checkpoint/store.rs`, `manager.rs`) | portable | builds | portable | **OK** (with caveats) | `git2 0.19, default-features=false, features=["vendored-libgit2"]` (`crates/biorouter/Cargo.toml:110`) — the **same** vendored libgit2 the workspace already cross-compiles for `biorouter-mcp` (`crates/biorouter-mcp/Cargo.toml:77`), so no new cross-compile surface. Caveats: exec bit is not a Windows concept (restore drops mode 755), and `CheckoutBuilder::force` cannot delete a file another process holds open (Windows mandatory locking). |
| **BR-65** managed policy path (`config/paths.rs:62-92`) | `/Library/Application Support/BioRouter/` | `%ProgramData%\BioRouter\` | `/etc/biorouter/` | **OK** | Exemplary three-arm `#[cfg(target_os=…)]` + a `not(any(...))` → `None` fallback. |
| **BR-65** managed-file trust check (`managed/trust.rs`) | uid + mode | degraded: **no-op** (`Ok(())`) | uid + mode | **GAP (documented)** | `#[cfg(not(unix))] verify_trusted() -> Ok(())` (`trust.rs:55-58`), with an honest comment: ACL verification deferred. `libc` is correctly `[target.'cfg(unix)'.dependencies]` (`crates/biorouter/Cargo.toml:129-131`). |
| **BR-65** hook command runner | `sh -c` | `cmd /C` | `sh -c` | **OK** | `hooks/command_runner.rs:38-42` (pre-existing, dual-armed). |
| **BR-17** chat FTS (`session/chat_fts.rs`, `session_manager.rs:34`) | portable | portable | portable | **OK** | New `CREATE VIRTUAL TABLE … USING fts5`. Same statically-linked bundled SQLite on all 3 targets (sqlx → libsqlite3-sys `bundled`), so FTS5 availability cannot diverge by platform. `sanitize_fts_query` is pure string work. **Verify** the bundled build isn't shadowed by a system libsqlite3 in the Linux docker image (see "How to verify"). |
| **BR-44** undo history (`developer/undo_history.rs`) | portable | degraded: POSIX redirect syntax only | portable | **GAP (minor)** | `redirect_targets()` (`undo_history.rs:184+`) parses `>`, `>>`, `2>`, `&>`. PowerShell `Out-File` / `Set-Content` / `Tee-Object` writes are never snapshotted → `undo_edit` silently doesn't cover them on Windows. Persistence path is `etcetera`-based (`undo_history.rs:156-163`) — portable. |
| **BR-23** secret guard (`biorouter-mcp/src/secret_guard.rs`) | portable | portable | portable | **OK** | `has_separator()` checks **both** `/` and `\` (`secret_guard.rs:219-221`). Deliberately correct. |
| **BR-1** workspace summary (`agents/workspace_summary.rs`) | portable | portable | portable | **OK** | `ignore::WalkBuilder`, `Path::file_name()`, `canonicalize` as a cache key. Renders `name` + `/` for dirs — separator-agnostic. |
| **BR-2** context budget (`context_budget.rs`) | portable | portable | portable | **OK** | Pure string/token math, no OS surface. |
| **BR-42** active-work dashboard (`biorouter-mcp/src/active_work.rs`) | portable | portable | portable | **OK** | Pure in-memory registry. |
| Guardrails / tool-output (`guardrails/tool_output.rs`), tool monitor (`tool_monitor.rs`) | portable | portable | portable | **OK** | Pure. No OS surface. |
| MCP extension spawn (`agents/extension_manager.rs:236-238`) | pgroup | no-window | pgroup | **OK** | `#[cfg(unix)] command.process_group(0)` uses tokio's inherent unix-only method — no `CommandExt` import needed; `configure_command_no_window()` handles Windows. |
| CLI TUI (`biorouter-cli/src/session/tui/mod.rs`) | portable | portable | portable | **OK** | Kitty `KeyboardEnhancementFlags` are `#[cfg(unix)]`-gated at import **and** at all 3 use sites (`tui/mod.rs:24, 82, 106, 131`) — correct; conhost doesn't speak the protocol. |
| Agent Drafter bundle `which()` (`agent_drafter/bundle.rs:447-465`) | portable | portable | portable | **OK** | `#[cfg(windows)]` also probes `.cmd` / `.exe` (`bundle.rs:456-462`). Correct. |
| Shell selection (`developer/shell.rs:16-67`) | `$SHELL -c` | pwsh → powershell → cmd | `$SHELL -c` | **OK** | Correct three-way detection. This is precisely why the POSIX-only denylist is a gap: the tool *does* run PowerShell on Windows. |
| Retry / on-failure command (`agents/retry.rs:210-219`) | portable | `cmd /C` | portable | **OK** | Dual-armed. |

## BREAK findings (must fix before shipping)

**None.** Every `#[cfg]` gate in the campaign diff has a complementary arm or a portable fallback; every unix-only API is behind a gate whose dependency is itself target-gated. Specifically verified as *not* broken:

- `crates/biorouter/src/managed/trust.rs:20` — `libc::geteuid()` under `#[cfg(unix)]`, and `libc` is declared **only** under `[target.'cfg(unix)'.dependencies]` in `crates/biorouter/Cargo.toml:129-131`. The `#[cfg(not(unix))]` arm at `trust.rs:55` keeps the symbol defined on Windows.
- `crates/biorouter-mcp/src/developer/shell.rs:3-5` — `use std::os::unix::process::CommandExt` is `#[cfg(unix)]`-gated at the import. `which` is a real (workspace) dependency of `biorouter-mcp` (`crates/biorouter-mcp/Cargo.toml:51`), so the `#[cfg(windows)] detect_windows_shell()` path at `shell.rs:34-67` compiles.
- `crates/biorouter-mcp/src/developer/background.rs` — `libc` is an unconditional dep of `biorouter-mcp` (`Cargo.toml:69`) but only *used* under `#[cfg(unix)]`; the `libc` crate itself builds on `windows-gnu`.
- `crates/biorouter-sandbox/src/seatbelt.rs` — pure string/argv construction; the only macOS-only code is the `#[cfg(target_os="macos")]` live-enforcement **tests** (`seatbelt.rs:232, 276`). `available()` (`:169`) is a runtime check, not a compile gate, so the module builds everywhere.
- `crates/biorouter/src/checkpoint/` — `git2` with `vendored-libgit2` and `default-features = false` (no openssl/ssh backends), already proven on all three targets by `biorouter-mcp`.
- Every hardcoded POSIX path (`/tmp/...`, `/bin/sh`, `/etc/hosts`) in the diff is inside `#[cfg(test)]` code or is the `SANDBOX_EXEC` const (`seatbelt.rs:21`), which is only dereferenced when `available()` already returned macOS.

## GAP findings — ranked by user impact

### GAP-1 — On Windows the catastrophic-command safety net is effectively absent

> **Resolved** by BR-68 in Wave 3. See [Cross-platform command safety](command-safety.md).

**Files:** `crates/biorouter/src/security/patterns.rs:463-537` (BR-20 rules), `crates/biorouter/src/security/policy/baseline.policy.yaml:17-231` (BR-21 rules).
**Impact: HIGH. This is a real, shippable safety hole, not a cosmetic one.**

Both layers run on Windows (the code is platform-independent, `SecurityManager::catastrophic_blocks` at `security/mod.rs:92` is unconditional), but **every rule is Unix-shaped**:

- BR-20's 8 rules (`patterns.rs:463-537`): `rm_rf_root`, `mkfs_device`, `dd_raw_disk`, `fork_bomb`, `chmod_777_root`, `git_push_force_protected`, `curl_pipe_root_shell`, `system_power_off`. Of these, only `git_push_force_protected` and `system_power_off` are meaningful on Windows.
- BR-21's 10 baseline rules (`baseline.policy.yaml`): `binary: [rm|curl|wget|dd|wipefs|chmod|chown|shutdown|init]` with `path_glob: ["/", "/etc/**", "/usr/**", …]`. Only `baseline.system_power_off` (`:194-211`) can fire on a Windows box.

Meanwhile `ShellConfig::default()` correctly selects **PowerShell 7 → Windows PowerShell → cmd.exe** on Windows (`crates/biorouter-mcp/src/developer/shell.rs:34-67`). So the agent *is* handed a PowerShell, and:

| Windows-destructive command | Blocked today? |
|---|---|
| `Remove-Item -Recurse -Force C:\` | **No** — `RM_INVOCATION` (`patterns.rs:519-520`) requires a `\brm\b` token. |
| `rm -Recurse -Force C:\` (PowerShell aliases `rm`→`Remove-Item`) | **No** — the flags *do* match `RM_RECURSIVE_FLAG`/`RM_FORCE_FLAG` (`patterns.rs:521-524`), but `ROOT_OR_HOME_TARGET` (`patterns.rs:526-528`) only recognises `/`, `~`, `$HOME`, `${HOME}` — never `C:\`. |
| `del /f /s /q C:\*` · `rd /s /q C:\` | **No** — no rule mentions `del` or `rd`. |
| `format C: /q` | **No** — `format_drive` at `patterns.rs:72` is in the *soft* `THREAT_PATTERNS` list (off unless `SECURITY_PROMPT_ENABLED`), not the always-on floor. |
| `vssadmin delete shadows /all` (destroys restore points — the classic ransomware step) | **No** |
| `cipher /w:C:` · `diskpart` · `bcdedit /delete` · `reg delete HKLM\… /f` · `takeown /f C:\ /r` | **No** |
| `Set-ExecutionPolicy Bypass` + `iex (New-Object Net.WebClient).DownloadString(...)` | **No** in the floor — the only PowerShell pattern (`patterns.rs:101-103`, `powershell_download_exec`) is a soft `THREAT_PATTERN`, and its regex demands the literal token `powershell` **and** `DownloadString` **and** `Invoke-Expression`, so `iex(iwr …)` evades it. |

Note the asymmetry that makes this worse than a plain feature gap: the rules **do** load, the self-tests **do** pass, and the UI reports the policy engine as *on*. A Windows user is told they are protected.

**Recommended fix (does not require new abstractions):** add a Windows rule block to `baseline.policy.yaml` gated by nothing (the rules are harmless on POSIX, where `Remove-Item` never appears) — `binary: [Remove-Item, rm, del, rd, rmdir]` + an `arg_regex` for `-Recurse`/`-Force`/`/s`/`/q` + a `path_glob` accepting `?:/**` and `?:/Windows/**`; plus `format`, `vssadmin`, `cipher`, `diskpart`, `bcdedit`, `takeown`, `reg delete`. And add a Windows-drive alternative to `ROOT_OR_HOME_TARGET` (`patterns.rs:526`) and to `ROOT_SLASH_TARGET` (`patterns.rs:536`). Each rule carries its own embedded `tests:`, so the coverage is self-enforcing.

### GAP-2 — Windows orphan reaper has no PID-reuse guard and can force-kill an innocent process tree

> **Resolved** in Wave 3 by commit `3d6d3aa9`, which added a `same_process` fingerprint
> before any force-kill. See the
> [cross-platform cluster verification report](parity-verification-report.md).

**File:** `crates/biorouter-mcp/src/developer/background.rs:502-506`.

```rust
#[cfg(windows)]
fn is_group_leader(_pid: u32) -> bool {
    // No cheap pgid equivalent on Windows; rely on liveness + `taskkill /T`.
    true
}
```

`reap_orphans_in()` (`background.rs:415-459`) reaps a recorded child when `pid_alive(child) && is_group_leader(child)`. On Unix the group-leader check (`background.rs:486-500`, `ps -o pgid=`) is *the* PID-reuse guard — the comment at `:481-485` says so explicitly. On Windows that guard is `true`, so the only protection left is `pid_alive`, which is exactly the condition a *reused* PID satisfies. The consequence: a stale pid file from a crashed daemon plus a recycled PID (Windows recycles PIDs far more aggressively than Linux) → `taskkill /F /T /PID <n>` (`background.rs:516-521`) force-kills an unrelated process **and its whole child tree**.

**Fix:** record the child's process **creation time** (or a `\BaseNamedObjects` Job Object handle) alongside the pid in the pid file, and only reap when it still matches. Cheapest stopgap: also persist the job's command line at `record_pidfile_in` (`background.rs:399-404`) and compare it against `wmic process where processid=<n> get commandline` / `tasklist /FI` before killing.

**Secondary, same file:** `pid_alive` on Windows (`background.rs:471-479`) shells out to `tasklist` **once per pid file** on every `BackgroundJobs::new()`. On a machine with a few stale records that's several process spawns on the agent's startup path.

### GAP-3 — BR-21's argv tokenizer mangles every Windows path

> **Resolved** by BR-68 in Wave 3, which added the dialect-aware `pwsh.rs` and
> `cmd_shell.rs` tokenizers. See [Cross-platform command safety](command-safety.md).

**File:** `crates/biorouter/src/security/policy/command.rs:195`.

```rust
shlex::split(stage).unwrap_or_else(|| stage.split_whitespace().map(String::from).collect());
```

`shlex` implements **POSIX** shell word-splitting: backslash is an escape character. `C:\Users\me\repo` tokenizes to `C:Usersmerepo`, and `normalize_path()` (`command.rs:319-343`) then treats that as a *relative* path and joins it onto the session cwd. So on Windows the engine's "resolved path" — the thing `path_glob` matches against, and the thing that folds `..` traversal (`command.rs:328-337`) — is garbage for every absolute path the user actually types.

`Glob::matches_path` (`security/policy/rule.rs:152-155`) is *correctly* written for this world (it normalizes `\` → `/` before matching), which makes the tokenizer the sole defect. Today this is masked by GAP-1 (no Windows rule has a `path_glob` that would match anyway) — but it means any Windows rules added to fix GAP-1 **will not work** until the tokenizer is fixed. Fix them together: `#[cfg(windows)]` (or a runtime flag) select a `\`-preserving tokenizer.

`home_dir()` (`command.rs:357-362`) is already correct — it falls back to `USERPROFILE`. Good.

### GAP-4 — BR-64: the shell sandbox exists on exactly one of three shipped platforms

> **Resolved** by BR-69 in Wave 3, which introduced the `ShellSandbox` trait and the
> Linux and Windows backends. See [Linux and Windows sandboxing](linux-and-windows-sandboxing.md).

**Files:** `crates/biorouter-sandbox/src/seatbelt.rs:167-170`, `crates/biorouter-mcp/src/developer/shell.rs:125-157`.

`available()` = `cfg!(target_os = "macos") && Path::new("/usr/bin/sandbox-exec").exists()`. On Windows and Linux, `shell_sandbox_wrap()` logs

> `BIOROUTER_SHELL_SANDBOX is set but no OS sandbox is available on this host; running the shell tool unsandboxed`

and returns `None` (`shell.rs:135-141`) — i.e. **fail-open**. This is defensible for Slice 1 (opt-in, default off, `BIOROUTER_SHELL_SANDBOX` unset ⇒ nothing changes), and the code is honest about it. But it must not be described in release notes as "BioRouter now sandboxes the shell" without the platform qualifier.

Two follow-ons worth queuing, in impact order:

1. **Linux:** `bubblewrap` (`bwrap --ro-bind / / --bind <ws> <ws> --unshare-net`) or Landlock — a near-exact map of the Seatbelt model (full read, writable roots, no net). Landlock needs kernel ≥ 5.13; the deb/rpm glibc floor (Debian 12 / Ubuntu 22.04, per `CLAUDE.md`) clears that comfortably.
2. **Windows:** AppContainer / restricted token — a much larger lift; realistically "no sandbox" stays true on Windows for a while. Say so explicitly rather than fail-open silently.

The design itself is sound and I found no defects: paths are injected as `-DWRITABLE_ROOT_n=` argv elements so a path can never inject SBPL syntax (`seatbelt.rs:144-159`); roots are `canonicalize()`d so macOS's `/var/folders` → `/private/var/folders` symlink doesn't spuriously deny in-root writes (`seatbelt.rs:152-155`); and zero roots emits **no** write block rather than a bare `(allow file-write*)` (`seatbelt.rs:119-130`) — the inverted-default bug it would have been.

### GAP-5 — Windows background jobs get no graceful-shutdown window

> **Partly resolved.** The graceful Windows kill phase shipped with commit `3d6d3aa9`
> in Wave 3; the Job Object recommendation below is not recorded as shipped.

**File:** `crates/biorouter-mcp/src/developer/background.rs:343-359`.

Unix: `SIGTERM`, then `SIGKILL` after 1500 ms — a job mid-write gets a chance to flush. Windows: straight to `taskkill /F /T` (force). A `shell_kill` on a long-running Windows build/export can truncate its output file. Consider `taskkill /T /PID` (no `/F`) first, then `/F` after the same 1.5 s.

**Related, same file:** background jobs are never placed in a Windows **Job Object** and `configure_shell_command` only sets a process group `#[cfg(unix)]` (`shell.rs:198-201`). `taskkill /T` walks the live PPID tree, so any grandchild that outlives its parent (a detached `cmd.exe` spawning a service, say) is missed — whereas a Unix pgid survives the leader's death. Assigning the child to a Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` is the real fix and would also close GAP-2.

### GAP-6 — BR-44 `undo_edit` doesn't see PowerShell writes

> **Appears open.** No remediation is recorded in the Wave 3 cluster.

**File:** `crates/biorouter-mcp/src/developer/undo_history.rs:184+` (`redirect_targets`).

Recognises `>`, `>>`, `1>`, `2>`, `&>`, `>|` and their fused spellings. PowerShell honours `>`/`>>` too, so the common case is covered — but `Out-File`, `Set-Content`, `Add-Content`, `Tee-Object` and `New-Item -Value` are the *idiomatic* PowerShell writes and none are detected. Result: on Windows, `undo_edit` silently has no snapshot for the file the agent just clobbered. Low blast radius (missing an undo entry never breaks a command, per the module's own doc comment at `:180-183`) but it's a quiet loss of the campaign's recovery net on one platform.

### GAP-7 — BR-65 managed-policy tamper check is a no-op on Windows

> **Appears open.** No remediation is recorded in the Wave 3 cluster.

**File:** `crates/biorouter/src/managed/trust.rs:52-58`.

`verify_trusted()` returns `Ok(())` unconditionally on non-Unix. The comment is honest ("`%ProgramData%` is admin-writable-only by default … deferred to phase 2"), and the *default* ACL on `%ProgramData%\BioRouter\` does restrict writes to admins — **but only if the directory was created by an elevated installer**. If BioRouter (or an MDM script) creates `%ProgramData%\BioRouter\` while running as the user, the directory inherits a user-writable ACE, and the enterprise policy file the whole BR-65 tier depends on becomes user-rewritable with no check. Worth an explicit `GetNamedSecurityInfo` owner + DACL check via the `windows` crate before BR-65 is marketed as an enterprise control.

### GAP-8 — BR-43 checkpoint restore on Windows: exec bits and locked files

> **Appears open.** No remediation is recorded in the Wave 3 cluster.

**File:** `crates/biorouter/src/checkpoint/store.rs:175` (`CheckoutBuilder`).

Two Windows-only degradations, both inherent to git-on-Windows rather than campaign defects, but neither is documented:

- **Exec bit:** blobs are stored with mode `100755`/`100644`; Windows has no exec bit, so a `/rewind` on Windows silently drops the executable bit on restored scripts. (The shadow repo does not set `core.fileMode = false`, so a mac/linux checkpoint restored on Windows and re-checkpointed will also show phantom mode churn.)
- **Locked files:** `CheckoutBuilder::force` + the "delete files created since the checkpoint" logic cannot remove a file another process holds open (Windows enforces mandatory locking; POSIX does not). A rewind while a dev server holds `dist/app.js` open partially fails. Worth surfacing as a user-visible warning rather than a silent partial restore.

## OK findings (verified portable)

These are the matrix's **OK** rows, expanded with the `file:line` evidence behind each
verdict.

- `security/policy/rule.rs:152-155` — `Glob::matches_path` normalizes `\` → `/` before matching. Deliberately cross-platform. (Its value is currently latent; see GAP-3.)
- `security/policy/command.rs:357-362` — `home_dir()` tries `HOME` then `USERPROFILE`.
- `biorouter-mcp/src/secret_guard.rs:219-221` — `has_separator()` checks `/` **and** `\`.
- `config/paths.rs:62-92` — three explicit `#[cfg(target_os = …)]` arms plus a `not(any(...))` → `None` fallback. Model of how to do this.
- `crates/biorouter/Cargo.toml:129-131` — `libc` under `[target.'cfg(unix)'.dependencies]`, matching the `#[cfg(unix)]` use in `managed/trust.rs`. No dead Windows dep, no missing symbol.
- `developer/shell.rs:16-67` — pwsh → powershell → cmd.exe detection with `-NoProfile -NonInteractive`.
- `developer/shell.rs:210-258` — foreground `kill_process_group`: Unix SIGTERM→SIGKILL; Windows `taskkill /F /T` **awaited before** `child.kill()`. Ordering is right.
- `agents/retry.rs:210-219` — `cmd /C` vs `sh -c`, dual-armed; the timeout test at `:394-399` also picks a per-platform command.
- `agents/extension_manager.rs:236-238` — `#[cfg(unix)] process_group(0)` (tokio inherent unix-only method) + `configure_command_no_window()` for Windows.
- `agent_drafter/bundle.rs:447-465` — `which()` probes `.cmd`/`.exe` under `#[cfg(windows)]`.
- `biorouter-cli/src/session/tui/mod.rs:24, 82, 106, 131` — kitty keyboard-enhancement flags gated `#[cfg(unix)]` at import *and* every push/pop site (including the panic hook and `Drop`), so no unbalanced state on Windows.
- `session/chat_fts.rs` (whole file), `context_budget.rs`, `guardrails/tool_output.rs`, `tool_monitor.rs`, `active_work.rs`, `agents/workspace_summary.rs` — pure logic / `ignore`-crate walks / `etcetera` dirs. No OS assumptions. `workspace_summary` renders `file_name()` + `/`, never a raw path string, so its tree output is identical on all platforms.
- `checkpoint/store.rs:44-75` — bare shadow repo with an externally-bound work-tree (`set_workdir(.., false)`), so it never writes a `.git` into the user's tree. Uses `Component::Normal` matching (`:239`) rather than string `"/.git/"` — separator-agnostic.
- No `/proc` read was **added** by the campaign. `rmcp_developer.rs:1743` (`/proc/1/cgroup`) is pre-existing container detection with a `let Ok(...) else` fallback; `security/policy/baseline.policy.yaml:38` (`/proc/**`) is a deny *pattern*, not a filesystem read.
- No hardcoded POSIX path exists in non-test campaign code, other than `SANDBOX_EXEC` (`seatbelt.rs:21`), which is only used after `available()` has confirmed macOS.

## How to verify

> **Now largely shipped.** BR-70 encoded this section's recommendations as
> `scripts/cross-env.sh`, the `check-cross` / `check-cross-linux` / `check-cross-windows`
> Justfile recipes, and `.github/workflows/rust.yml`. Prefer those over the ad-hoc docker
> commands below; see [the CI gate design](ci-gate.md) for what each one does and why.

### Linux (docker cross-build — the same image the release pipeline uses)

```bash
# Pinned image (glibc 2.31 floor; see LINUX_RUST_IMG in scripts/release.sh)
docker run --rm -v "$PWD":/src -w /src rust:1.92-bullseye bash -lc '
  apt-get update -qq && apt-get install -y -qq cmake pkg-config libssl-dev >/dev/null
  export LZMA_API_STATIC=1
  cargo check  --workspace --all-targets
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test -p biorouter --lib security::            # BR-20/BR-21 rule self-tests
  cargo test -p biorouter --lib checkpoint::          # BR-43 git2 shadow repo
  cargo test -p biorouter --lib session::             # BR-17 FTS5 must exist in bundled sqlite
  cargo test -p biorouter-mcp --lib developer::background::   # BR-37/42 pgroup + reaper
  cargo test -p biorouter-sandbox
'
```

The `session::` line is the one that proves FTS5 is compiled into the *Linux* bundled SQLite — if libsqlite3-sys were ever resolved via `pkg-config` to a system `libsqlite3` built without FTS5, `CREATE VIRTUAL TABLE … USING fts5` (`session_manager.rs:34`) would fail there and only there. Assert it explicitly:

```bash
docker run --rm -v "$PWD":/src -w /src rust:1.92-bullseye bash -lc \
  'cargo tree -p biorouter -i libsqlite3-sys -e features | grep -q bundled && echo "FTS5: bundled sqlite OK"'
```

### Windows (cross-compile check + the pieces that need a real host)

```bash
# Compile check — catches BREAK-class issues (a missing cfg arm, an ungated unix import).
docker run --rm -v "$PWD":/src -w /src rust:1.92-bullseye bash -lc '
  rustup target add x86_64-pc-windows-gnu
  apt-get update -qq && apt-get install -y -qq gcc-mingw-w64-x86-64 cmake >/dev/null
  export LZMA_API_STATIC=1
  export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
  cargo check --workspace --all-targets --target x86_64-pc-windows-gnu
'
```

Note what this does **not** prove: `cargo check --target` compiles the `#[cfg(windows)]` arms but *runs* nothing. GAP-1, GAP-2, GAP-3, GAP-5 and GAP-6 are all behaviours that only a real Windows host (or a Windows CI runner) can exercise. Minimum bar on a `windows-latest` runner:

```powershell
cargo test -p biorouter      --lib security::
cargo test -p biorouter      --lib checkpoint::
cargo test -p biorouter-mcp  --lib developer::
cargo test -p biorouter-sandbox
```

…and then the tests that **don't exist yet** and should be written as part of fixing GAP-1/GAP-3:

- `Remove-Item -Recurse -Force C:\` and `rm -Recurse -Force C:\` are blocked by `match_catastrophic_command`.
- `del /f /s /q C:\*`, `format C:`, `vssadmin delete shadows /all` are blocked.
- `ParsedCommand::parse(r"rm -r C:\Users\me\repo", cwd)` yields a `paths` entry equal to `C:\Users\me\repo` (today it yields `<cwd>\C:Usersmerepo`).

### macOS (host)

```bash
source bin/activate-hermit
cargo test --workspace
# Live kernel-enforcement proof of BR-64 (macOS-only, skipped elsewhere):
cargo test -p biorouter-sandbox seatbelt_enforces_write_confinement -- --nocapture
```

### What CI should run on every PR

Today the campaign's platform arms are only compiled on the developer's Mac. Minimum to keep this document from going stale:

| Job | Runner | Command | Catches |
|---|---|---|---|
| `check-linux` | `ubuntu-latest` | `cargo clippy --workspace --all-targets -- -D warnings` | Linux BREAKs, `cfg(unix)` regressions |
| `check-windows` | `windows-latest` | `cargo clippy --workspace --all-targets -- -D warnings` | **the whole `#[cfg(windows)]` surface — currently compiled by nobody on every PR** |
| `test-windows` | `windows-latest` | `cargo test -p biorouter --lib security:: checkpoint:: session::` + `cargo test -p biorouter-mcp --lib developer::` | GAP-1/2/3/5/6 once rules + tests exist |
| `test-macos` | `macos-latest` | `cargo test --workspace` | BR-64 live enforcement |
| `check-versions` | any | `just check-versions` | (existing) |

A `windows-latest` **clippy** job is the single highest-value addition: it is cheap, it needs no new tests, and it is the only thing standing between the repo and a future PR that adds an ungated `std::os::unix::…` import and breaks the Windows zip at release time — a failure that today would surface for the first time inside `scripts/release.sh backends`.

## Related documentation

- [Cross-platform cluster verification report](parity-verification-report.md) — the Wave 3 gate record that cleared GAP-1 through GAP-4.
- [Cross-platform command safety (BR-68)](command-safety.md) — the design that remediated GAP-1 and GAP-3, restating both findings at length.
- [Linux and Windows sandboxing (BR-69)](linux-and-windows-sandboxing.md) — the design that remediated GAP-4.
- [Cross-platform CI verification gate (BR-70)](ci-gate.md) — what the "How to verify" recommendations became.
- [Improvement proposals](../../history/agent-loop-review/improvement-proposals.md) — the index defining every BR-NN cited here.
