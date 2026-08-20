# Debugging Biorouter on Windows

> **What this is.** How to diagnose a Windows-only failure in this repo when you are
> almost certainly working on a Mac, plus the currently-open Windows defects and what
> has already been ruled out for each.
> **Status:** Current. The open-defect section is a snapshot of 2026-08-20 and is
> expected to shrink; the technique sections are durable.
> **Audience:** developers and agents debugging a Windows CI failure or a Windows-only
> bug report.

Nobody on this project develops on Windows, so every Windows problem is diagnosed at
one remove. That changes the method: you cannot bisect by running the thing. What you
have instead is CI, a cross-compiler, and the discipline of reading evidence rather
than guessing — and this page is mostly about not wasting a day guessing.

## Start by asking whether it failed or hung

**This is the first question, and it is easy to get wrong.** A hung Windows job looks
like a slow one, and "CI is slow tonight" is a story that fits the evidence and is
wrong. It cost several hours on 2026-08-20.

The tell is not the absolute time. It is the time **compared with the job's own
history**:

```bash
gh run list --workflow rust.yml --limit 25 --json databaseId,headSha,conclusion,status
gh run view <run-id> --json jobs   # read startedAt / completedAt per job
```

On this repo `test (windows-latest)` finishes in **20–41 minutes**. When it sat at 120+
minutes it was hanging, and it had been hanging on every merged head — while each
contributing branch had passed Windows individually in about 20. A defect that appears
only after a merge is invisible to every branch's own CI, which is exactly why the
duration comparison is worth doing before anything else.

A hang is worse than a failure in one specific way: **it hides every test after it.**
Fixing the hang on 2026-08-20 immediately revealed five real failures that had never
once been observed.

### Why a Windows test hangs rather than fails

The usual cause is a child process that never answers, awaited without a timeout.
Two ingredients, both present here:

- **A test spawns something Windows does not have.** `python3` is the recurring one:
  there is no `setup-python` step in [`.github/workflows/rust.yml`](../../.github/workflows/rust.yml),
  so on the Windows runner `python3` resolves to the Microsoft Store's App Execution
  Alias, or to nothing at all.
- **The await has no deadline.** `AppServer::request`
  (`crates/biorouter/src/providers/coding_agent/appserver.rs`) documents that it has no
  timeout of its own — a real turn is bounded by `TURN_TIMEOUT` in the *provider*, not
  by the transport. A test that calls `request()` directly therefore waits forever.

So a missing interpreter does not fail the job. It hangs it to the runner's ceiling.
Tests that depend on a POSIX-shaped fake process are marked `#[cfg(unix)]` for this
reason; if you add one, mark it, or give it a timeout.

## What you can actually run from a Mac

| You want to check | Command | Catches |
|---|---|---|
| It still compiles for Windows | `cargo check --target x86_64-pc-windows-gnu` | type errors, `cfg` mistakes, dangling references after gating |
| It compiles including tests | add `--tests` to the above | a helper you gated but whose caller you did not |
| It links and packages | `scripts/release.sh windows <ver>` (Docker) | the mingw link fixes below |
| It runs | **nothing local — only CI** | everything else |

`cargo check --target x86_64-pc-windows-gnu` needs mingw on the host and will die with
`failed to find tool "x86_64-w64-mingw32-gcc"` without it. The release path avoids this
by cross-compiling in Docker; for a quick syntax/`cfg` check, prefer pushing to a branch
and reading CI's own `cross-check (x86_64-pc-windows-gnu)` job, which is fast (~8 min)
and runs on every push.

**Reading a failure out of CI:**

```bash
gh run view <run-id> --log-failed | grep -iE "panicked|FAILED|assertion|error\[" | head -40
```

`--log-failed` works only once the job has finished. There is no way to read a live log
for an in-progress job, which is another reason a hang is expensive.

## Windows traps that have actually bitten this repo

- **A file that is open cannot be deleted.** This is the single biggest behavioural
  difference from macOS and Linux, and it is the root of the ephemeral-store defect
  below. Anything that creates a directory, opens a handle into it (a SQLite pool
  especially), and then removes the directory needs the handle closed *first* —
  retrying the delete does not help, because the handle is still open.
- **Never route an absolute path through `cmd.exe`** (noted in `CLAUDE.md`; it was one
  of four defects an adversarial review found).
- **`HOME` is not defined.** Windows uses `USERPROFILE`. A bare
  `env::var("HOME").expect(...)` panics on any runner without a Git Bash session;
  `crates/biorouter-server/src/test_sandbox.rs` reads both names and documents why.
- **Cross-compile link fixes** live in the Justfile and `scripts/release.sh`:
  `aws-lc-sys` needs winpthread appended *after* the rlibs on the mingw link line, and
  `lzma-sys` needs `LZMA_API_STATIC=1`. Run the Docker cross builds with the system
  Docker — hermit does not shadow it.
- **After any Windows or Linux Docker build**, `ui/desktop/node_modules` is
  Linux-flavoured. Restore it with `npm ci` — **never `npm install`**, which rewrites
  the lockfile and breaks the next cross build.
- **Secrets are chunked** across multiple Windows Credential Manager entries (2560-byte
  cap each); see [secret storage](../security/secret-storage.md).
- **Windows has no in-place updater.** It ships as a plain zip and uses the assisted
  download fallback, so an auto-update problem reported on Windows is a different code
  path from macOS.

## Publication is gated on a native Windows smoke run

Nothing in the release pipeline executes the Windows build *on Windows* — the zip is
cross-compiled and packaged in Docker. That gap is why `scripts/release.sh publish`
refuses unless `release-artifact-smoke.yml` has a successful run titled
`Release artifact smoke v{ver}`. Treat that run as the only evidence the Windows
artifact actually starts.

## Open Windows defects

Snapshot of 2026-08-20, on `main` at `efe6cb41`. All five were invisible until the hang
above was fixed, so none of them is a fresh regression — they are newly *visible*.

### Four bridge tests: `Tool 'developer__shell' not found`

```
providers::coding_agent::bridge::tests::a_pretooluse_rewrite_decides_what_a_bridged_call_runs
providers::coding_agent::bridge::tests::a_rewritten_call_is_re_judged_by_the_security_floor
providers::coding_agent::bridge::tests::a_bridged_call_leaves_another_calls_rewrite_staged
providers::coding_agent::bridge::tests::concurrent_bridged_calls_each_run_their_own_rewrite
```

Each builds an `ExtensionManager`, adds the `developer` builtin, and dispatches
`developer__shell`. On Windows the dispatch returns `-32002: Tool 'developer__shell'
not found`.

**Ruled out, with evidence:**

- *The tool is platform-gated.* It is not — `#[tool(name = "shell", …)]` in
  `crates/biorouter-mcp/src/developer/rmcp_developer.rs` is unconditional.
- *Builtins spawn a subprocess that fails to resolve on Windows.* They do not —
  `ExtensionConfig::Builtin` loads **in-process** over `tokio::io::duplex` pipes and a
  `spawn_server` call (`crates/biorouter/src/agents/extension_manager.rs`), with no
  child process involved.
- *A pre-existing breakage.* No — all four tests were introduced by `b153fb50` and
  `c76a2a4d`, both in v1.89.2, so they had never run on Windows at all.

**Unproven leading hypothesis:** tool discovery is asynchronous, and the test dispatches
before the routed client has finished listing. That would be a slower-runner race rather
than a product defect, and it would mean the *product* is fine on Windows. **Do not act
on this without confirming it** — the cheap confirmation is to have the test await the
tool appearing in the manager's tool list before calling, push to a branch, and read CI.

### `close_ephemeral_store_removes_the_directory`

`crates/biorouter-cli/src/session/builder.rs:1189` — after `close_ephemeral_store`, the
`biorouter-no-session-*` directory still exists on Windows, so an early exit leaks it.

This is the Windows half of the open-handle rule above, and it is **not fixed**. Note
for anyone reading older session notes: this fix was once reported as confirmed by CI.
It was not — the Windows job was hanging on every merged head at the time and never
confirmed anything. Treat that earlier claim as withdrawn.

Related but distinct: on a developer machine `cargo test -p biorouter-cli` deadlocks
against a populated `~/.config/biorouter` (real knowledge bases, ~21 lock files) and
passes in 17 s against an empty one. To run that crate's tests locally at all, isolate
`HOME`, `XDG_CONFIG_HOME` and `BIOROUTER_PATH_ROOT` into a temp directory. CI never
sees it, because CI's config directory is empty. **That is a different bug from this
one** — same file, different cause — so do not conflate them.

## A checklist for the next Windows failure

1. Compare the job duration with its history. Hang or failure?
2. If it hung, find the child process that never answered, and the await with no
   deadline.
3. `gh run view <id> --log-failed` and read the *first* failure, not the loudest.
4. Ask whether the test has ever passed on Windows: `git log -S<test-name>` against the
   file, then check whether that commit's CI ever completed a Windows job.
5. Reproduce what you can on a Mac with `--target x86_64-pc-windows-gnu`, and be honest
   that it only proves compilation.
6. Change one thing, push to a branch, read CI. Resist fixing two things at once —
   Windows feedback costs 15–20 minutes a cycle and a combined change teaches you
   nothing.

## Related documentation

- [Launching the dev GUI](launching-the-dev-gui.md) — the equivalent procedure for
  driving the app locally, and the six ways it fails silently
- [Startup freeze and main-thread blocking](startup-freeze-and-main-thread-blocking.md)
  — the bundled MinGit copy was a Windows-first symptom
- [Window scaling regressions](window-scaling-regressions.md)
- [Secret storage](../security/secret-storage.md) — Windows Credential Manager chunking
- [Release process](../../RELEASE.md) and the Windows smoke gate in `CLAUDE.md`
