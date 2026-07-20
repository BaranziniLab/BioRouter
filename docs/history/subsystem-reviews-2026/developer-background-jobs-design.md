# Developer extension background jobs — design record

> **What this is.** The investigation, competitive survey, and shipped design for background process supervision in BioRouter: why the agent could not watch a long-running command, how other agents solve it, and the `shell background` / `shell_output` / `shell_wait` / `shell_kill` contract that was built in response — with its verification checklist and results.
> **Status:** Historical record — the design shipped on 2026-06-14 as part of the **developer** extension. The supervisor lives in `crates/biorouter-mcp/src/developer/background.rs`, the checklist below is filled in with passing results, and a live agentic run on 2026-06-14 confirmed the start → wait → report loop end to end.
> **Audience:** maintainers working on the developer extension and the agent loop.

A capable agent that starts a dev server, build, test suite, training run or deploy
should keep watching it and resume when it finishes — without killing the job and
without declaring it done early. Before this work BioRouter could do neither. This
document records the investigation into why, the survey of how comparable agents
solve it, and the design that shipped.

> **Note.** The title of this document is *background jobs*, not *monitor*. The
> feature was prototyped as a standalone `monitor` extension and that extension was
> removed; the shipped artefact is background job support inside the developer
> extension. Earlier references to a "background-process monitor" mean this.

> **Reading order.** If you only want the shipped tool contract, skip to
> [Chosen design](#chosen-design). The problem statement, codebase investigation and
> competitive survey record how that contract was arrived at.

**Date:** 2026-06-14.

## The problem

When BioRouter launches something that finishes in minutes (a dev server, build,
test suite, training run, deploy), a good agent should keep watching it and
resume work when it's done — *without* killing the job or prematurely declaring
it finished. Today BioRouter does neither well.

## What the codebase did at the time

- **Shell tool has no timeout and no tracking.** `developer` shell runs a
  command to completion (`crates/biorouter-mcp/src/developer/rmcp_developer.rs`),
  with `kill_on_drop(true)` + a new process group. The prompt tells the agent to
  background long commands (`uvicorn main:app &`). A backgrounded process *does*
  survive the tool call (the parent shell exits, so `kill_on_drop` only kills the
  already-dead shell), **but nothing tracks, reads, health-checks, or reaps it.**
  The agent's only way to re-check is to issue another shell call immediately —
  there is no "wait, then look again" primitive.
- **The agent loop is strictly synchronous** (`crates/biorouter/src/agents/agent.rs`).
  A turn ends when the model stops requesting tools. There is **no yield/resume,
  no timer, no deferred ("still running") tool result, no wakeup.** Hitting
  `max_turns` stops and asks the user rather than auto-resuming.
- **The scheduler is cron-only** (`crates/biorouter/src/scheduler.rs`): every
  fire spawns a *fresh* `SessionType::Scheduled` session. It cannot run "once
  after N minutes" and cannot resume the originating session.
- **The one mature supervisor pattern** is the llama.cpp sidecar
  (`crates/biorouter/src/providers/llamacpp_sidecar.rs`): spawn + `kill_on_drop`,
  deadline-based `/health` poll, `SidecarStatus` snapshots with a rolling
  log-tail, and a cross-process PID-file orphan reaper. This is the template the
  background-job supervisor reuses.

**Conclusion:** no built-in supervisor exists; the primitives to build one
(subprocess supervision, status snapshots, log tailing) do.

## How other agents do it

> **Provenance.** This is a synthesis of published behaviour of other agent
> products as of June 2026. The underlying sourced reports were archived in the
> originating task transcript and are not available in this repository, so the
> individual claims below are not independently citable from here.

The serious implementations converge on one shape:

> **start in background → durable job ID → read "only new output since last
> check" + an explicit status → decide done-vs-running from the OS exit status
> (never from log heuristics) → terminate by ID.**

- **Claude Code:** `Bash(run_in_background:true)` → shell ID; `BashOutput`
  returns *only new* output + status; `KillShell`. The agent **polls** (~30 s);
  no completion callback.
- **OpenAI Codex CLI:** same shape — background tasks, `bash_output`,
  `kill_shell`, `Ctrl+B` monitor.
- **Devin:** runs synchronously up to a wait window, then **auto-backgrounds**
  with a shell ID and checks later; API status `working|blocked|finished`, polled
  with capped backoff (`min(backoff, 30)`).
- **Cline "Proceed While Running"**, **Warp** PTY-attach + completion
  notification, **Cursor Cloud Agents** SSE status stream.
- **General patterns:** capped exponential backoff + jitter; async/deferred tool
  results (MCP Tasks `tasks/get`, OpenAI `requires_action`); "poll for truth,
  listen for speed"; suspend-and-resume with a durable timer (LangGraph
  `interrupt()`, Temporal `sleep`/`wait_condition`) for long waits.

## Chosen design

Implemented as part of the **developer** extension (the default coding surface),
following the Claude Code / Codex shape where background tasks are a property of
the command-runner, not a separate extension. The shell tool gains a
`background` mode and three companion tools; the supervisor/registry lives in
`crates/biorouter-mcp/src/developer/background.rs` (`BackgroundJobs`), reusing the
foreground shell's hardened command builder (`configure_shell_command`) with
`kill_on_drop(false)` so the job survives the tool call.

> Originally prototyped as a standalone `monitor` extension; merged into
> `developer/shell` per the maintainer's decision so it's available by default
> (developer is the only default-on extension) and co-located with the shell
> guidance that tells the agent to background long commands. The standalone
> extension was removed.

| Tool | Behavior |
|------|----------|
| `shell{command, background:true, label?}` | Spawn in a background process group, return a durable `job_id` immediately. Survives across tool calls. (background defaults false = normal synchronous shell.) |
| `shell_output{job_id}` | Return status + **only new output since the last check** (advancing a cursor). Non-blocking. |
| `shell_wait{job_id,timeout_secs}` | Watch up to `timeout_secs` (default 120, max 600). Return the moment the job **exits**, or at timeout with `status=running` and guidance to call again. **Never kills the job**; the loop continues. |
| `shell_kill{job_id}` | SIGTERM then SIGKILL the whole process group; status → `killed`. |

### Invariants

- Done-vs-running is decided from the **OS exit code** (`child.wait()`), never
  from log text — this is exactly the failure mode that makes Windsurf hang.
- `shell_wait` returning `running` is the "report back without killing the
  job" behavior: the agent re-enters by calling `shell_wait` again (poll
  model), and the job keeps running in the registry meanwhile.
- Output is capped (400 KB) with an explicit truncation marker; whole process
  **group** is killed (not just the parent), reused from the shell-kill idiom.

## Open item: cross-turn suspend-and-resume

**Out of scope for the MVP (documented phase-2 path):** true cross-turn
suspend-and-resume (checkpoint the session, register a one-shot scheduler wakeup
that resumes the *same* session). That needs: a one-shot/delayed job in
`scheduler.rs`, a session-resume entry that injects a system message, and an
agent-loop yield point. The MVP's bounded `shell_wait` covers the common
minutes-scale case without that surgery.

This item was not delivered with the MVP. Treat it as outstanding unless a later
document supersedes it.

## Verification checklist and results

Backend unit tests — **all green** (`cargo test -p biorouter-mcp --lib developer::background::`, 6/6):

- [x] `shell background=true` returns a job id and the job appears in shell job list. (`start_lists_and_completes_with_output`)
- [x] A fast command (`echo hello`) reaches `exited(0)` and its output contains `hello`.
- [x] A failing command (`exit 3`) reaches `exited(3)` — non-zero surfaced. (`nonzero_exit_code_is_surfaced`)
- [x] `shell_output` is incremental: two checks never return the same bytes twice. (`output_is_incremental`)
- [x] `shell_wait` on a fast job returns completion before the timeout. (`wait_returns_early_on_completion`)
- [x] `shell_wait` on a long job (`sleep 30`) returns `running` after a short
      timeout **and the job is still alive afterward** (not killed). (`wait_times_out_without_killing_then_kill_works`)
- [x] `shell_kill` on a long job transitions it to `killed` and the process group dies. (same test)
- [x] Output beyond the cap is truncated with a marker (no unbounded memory). (`MAX_OUTPUT_BYTES` + truncation note)
- [x] Unknown `job_id` is a clean error, not a panic. (`unknown_job_is_clean_error`)

Integration / prompt:

- [x] background jobs live in the always-on `developer` extension (no separate toggle); `BackgroundJobs` registry; dependent `biorouter` crate compiles.
- [x] Extension instructions tell the agent the loop: start → wait/check → act on
      status, and to prefer `shell background=true` over `cmd &` (the `INSTRUCTIONS` const).
- [x] Available by default via the developer extension; the standalone monitor extension and its catalog entries were removed.

### Verification history

- The original standalone `monitor` extension was Playwright-verified in the dev
  app on 2026-06-14 (appeared as a BUILT-IN, enabled without error, and all five
  `monitor__monitor_*` tools registered with a live `/agent/tools` session). That
  prototype was then merged into the developer extension (this design) and removed.
- **Live agentic run (merged design), 2026-06-14:** with the local `llamacpp`
  qwen3.5-4b model, a headless run asked the agent to start a background job and
  report when it finished. The model called `shell(background=true)` then
  `shell_wait` and reported `exited(0)` with the job's output — start → wait →
  report, without running inline or killing the job.

## Related documentation

- [Developer extension](../../extensions/built-in/developer.md) — the living reference for the shell tools this design added to.
- [Long-running tasks and scheduling review](../agent-loop-review/subsystem-reviews/long-running-tasks-and-scheduling.md) — the broader review of the synchronous agent loop and cron-only scheduler that constrain the phase-2 path.
- [Scheduled jobs](../../workflows/scheduled-jobs.md) — the cron scheduler that a one-shot wakeup would have to extend.
- [Claude Code (agent landscape)](../../research/coding-agent-landscape/claude-code.md) — the background-task shape this design follows.
- [Core loop and tool dispatch review](../agent-loop-review/subsystem-reviews/core-loop-and-tool-dispatch.md) — where the agent-loop yield point discussed under the open item would live.
