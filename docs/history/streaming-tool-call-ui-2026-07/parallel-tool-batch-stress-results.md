# Parallel tool-call batches — stress and edge-case results

The campaign that owns this folder rebuilt how one assistant turn dispatches *many* tool calls: an
8-permit semaphore with per-path write locks (`tool_dispatch_limits.rs`), the MCP client mutex
removed (`63b7e493`), decoder batching so several `tool_use` blocks arrive as one message
(`8e20f6cc`), per-tool response emission in completion order (`ae740027`), and pending tool-call
events (`77a7564d`). Each landed with a gate for the behaviour it added. This round asks the
different question: **what happens to a wide batch when things go wrong** — a tool fails, the user
cancels, two tools write the same file, more tools are requested than there are permits.

Seven exercises, run against the real dispatch path rather than the units beneath it, because the
permit is acquired *inside* the tool future and every one of these invariants lives at the seam
where the loop, the persistence writer and the transcript meet. Two defects were found and fixed;
both are revert-proven, meaning the gate was watched failing before the fix and passing after.

Findings are numbered `PAR-NN`.

## Summary

| ID | Severity | Area | Status |
|---|---|---|---|
| [PAR-01](#par-01) | — | Wide batch: exactly-once execution, result-to-request mapping | PASS, gated |
| [PAR-02](#par-02) | **High** | `shell` discarded the exit status, so failures rendered green | **FIXED**, revert-proven |
| [PAR-03](#par-03) | — | Partial failure mid-batch does not cancel siblings | PASS, gated |
| [PAR-04](#par-04) | **High** | Cancelling mid-batch persisted `tool_use` blocks with no `tool_result` | **FIXED**, revert-proven |
| [PAR-05](#par-05) | — | 8-permit ceiling binds; `MAX_CONCURRENT=1` fully serializes | PASS, gated |
| [PAR-06](#par-06) | — | Both kill switches still select between two real behaviours | PASS, gated |
| [PAR-07](#par-07) | — | Same-path writers never interleave | PASS, gated |
| [PAR-08](#par-08) | Low | The concurrency lever is start-up-only, not runtime | DOCUMENTED |

## PAR-01 — a wide batch runs every tool exactly once {#par-01}

**PASS.** Six `developer__shell` calls in one assistant message, with staggered sleeps so completion
order differs from request order. Each appends a marker to its own file, so a tool dispatched twice
leaves two lines.

All six executed, each exactly once, each response carried its own output and no sibling's, and the
persisted transcript is six request-ordered `tool_use`/`tool_result` pairs. Arguments arrived intact
— the decoder batching in `8e20f6cc` does not truncate or cross-wire a wide batch.

Gate: `wide_batch_runs_every_tool_exactly_once_with_results_mapped_to_ids` in
`crates/biorouter/tests/parallel_tool_batch_stress.rs`.

## PAR-02 — a failed shell command reported success {#par-02}

**High severity. Fixed.**

Found while building the partial-failure gate: a tool that ran `exit 7` came back as
`Ok(CallToolResult)` with `is_error: false` and **empty text**. A command that failed silently was
byte-for-byte indistinguishable from one that succeeded quietly.

The cause was one line in `crates/biorouter-mcp/src/developer/rmcp_developer.rs`:

```rust
let _exit_status = child.wait().await...;
```

The exit status was awaited and thrown away, and the result was built with
`CallToolResult::success(...)` unconditionally. Two things follow, and both matter:

- **The UI renders the failure green.** `dfa6dc32` earlier in this campaign fixed the frontend to
  read the `isError` flag so failed calls stop rendering as successes — but for the single most-used
  tool in the app that flag was never set, so the frontend fix could not bite.
- **The model cannot see the failure either.** With no stderr and no status, a failed build or a
  failed test run reaches the model as an empty successful result, and it proceeds as if the step
  worked.

This is not a judgement call about what *ought* to happen. The tool's own description, sent to the
model on every turn, promises: *"There will also be an indication of if the command succeeded or
failed."* There was none. The tool did not honour its documented contract.

**Fix.** `execute_shell_command` now returns `(String, Option<i32>)` — output plus exit code — and
the `shell` tool appends an explicit status line for a non-zero exit and sets `is_error`:

```
[shell: command exited with status 7]
```

Exit 0 is untouched, so nothing changes for the overwhelmingly common case. A signal-terminated
process (`code() == None`) is reported as a failure too, since it certainly did not succeed. The
status line is appended to both audience copies, so the model and the user see the same thing, and
it is the *whole* body when the command printed nothing — which is exactly the silent-failure case
that was invisible before.

**Known behaviour change, deliberate:** commands that use a non-zero exit as ordinary signalling —
`grep` with no match, `diff` on differing files, `test` — now surface as error cards. That is the
honest reading of the contract and of the exit code, and the status line names the code so the model
can tell "no match" from "broke". Flagged here because it is the one user-visible consequence of
this fix worth watching in the field.

**Revert proof.** Before the fix the gate failed with
`("call_boom", "boom-stderr\n", false)` — `is_error` false. After, it passes.

Gate: the `is_error` and `status 7` assertions in `a_failing_tool_does_not_cancel_its_siblings`.

**Blast radius: exactly one existing test**, and it argued for the fix rather than against it.
`case15_tool_error_propagates_as_js_exception` runs `cat /nonexistent/path` inside `execute_code`
and asserts, in its own words, that we must not see "a silent success that pretends the file was
read". It began failing because the failure now propagates all the way out as `execute_code`'s own
error flag — the outcome the case exists to demand. It had been written against the asserting `exec`
helper, which blanket-forbids an error result; the same file already provides `exec_raw` "for cases
where an error result is the expected outcome". Moved to `exec_raw` and strengthened to assert the
error propagates *and* names what failed.

The companion `case16_try_catch_recovers_from_tool_error` passed untouched, which is the useful
control: an error the JS catches still leaves `execute_code` successful, so the new flag propagates
through real failures without poisoning handled ones.

## PAR-03 — a failing tool does not cancel its siblings {#par-03}

**PASS.** A three-call batch whose middle call fails fast (`exit 7`) while two slower siblings are
still running. The failure surfaces as `is_error` (after PAR-02), both siblings complete and write
their side effects to disk, and the turn continues to the model's follow-up reply. The persisted
batch is three matched pairs — a partial failure leaves a complete, replayable record.

Gate: `a_failing_tool_does_not_cancel_its_siblings`.

## PAR-04 — cancelling mid-batch persisted unmatched `tool_use` blocks {#par-04}

**High severity. Fixed.**

Cancelling a turn while a batch is in flight left the session in a state no provider will replay.
The batch loop breaks out of `select_all` the instant the cancel token trips, abandoning every tool
that has not yet returned — correct, that is what cancelling means. But the post-batch persistence
loop still writes a `tool_use` for **every** request in the batch, and the abandoned ones had only
their empty placeholder response, which is dropped as an empty message. The result:

```
[("req","call_quick"), ("resp","call_quick"), ("req","call_slow_a"), ("req","call_slow_b")]
```

Two `tool_use` blocks with no `tool_result`. An assistant turn carrying an unmatched `tool_use` is
rejected outright by Anthropic and others, so the saved session is corrupt at rest.

In practice `fix_conversation`'s `fix_tool_calling` pass repairs this on the way *out* to a provider,
which is why it had not been seen as a live 400. That is a safety net, not the contract: the stored
record is still wrong, and anything reading the session raw — export, replay, analysis, a future
consumer that does not route through the normalizer — sees the orphans.

The invariant was already known and already enforced everywhere else. `tool_execution.rs` carries a
helper whose doc comment states it plainly: *"Every tool request must end with a tool response — a
request with none breaks the next provider call"*, and declines, expiries and approval-stage cancels
all funnel through it. The mid-run cancel was the one path that did not.

**Fix.** After the batch loop, when the token is cancelled, every still-empty response slot is
backfilled with an explicit result (`CANCELLED_MID_RUN_RESPONSE`, `is_error: true`) telling the
model the call was interrupted, may have partially completed, and must not be assumed to have
succeeded. Slots that already hold a result are left untouched, so no completed tool's output is
overwritten. This mirrors exactly what chat mode already does for the calls it skips.

**Revert proof.** Before the fix the gate failed on the orphan block above. After, it passes.

Gate: `cancelling_mid_batch_leaves_no_unmatched_tool_use_persisted` in
`crates/biorouter/tests/parallel_tool_batch_cancellation.rs`. It also asserts the turn *ends*
promptly rather than waiting out the abandoned tools, that nothing is persisted twice, and that the
pairing is positional rather than merely set-equal.

## PAR-05 — the concurrency ceiling and the rollback lever {#par-05}

**PASS.** Twelve tools requested against eight permits. Each marks itself live, samples how many
tools are live at that moment into its own file, then clears the mark — private sample files, so the
per-path write locks cannot serialize the probes and confound the measurement.

Peak observed concurrency: **exactly 8**. The ceiling binds precisely, and the gate also asserts
peak > 1 so a change that accidentally serialized everything cannot pass by hiding under the cap.

With `BIOROUTER_TOOL_MAX_CONCURRENT=1`, every one of six probes observed exactly one live tool: the
documented rollback lever still fully serializes, and serializing loses no work.

Gates: `parallel_tool_batch_concurrency_cap.rs` and `parallel_tool_batch_serialization_lever.rs`.

## PAR-06 — both kill switches still work {#par-06}

**PASS.**

`BIOROUTER_TOOL_RESPONSE_STREAMING=0` restores pre-§6.2c ordering: the streamed transcript falls back
to request order (slow, fast) instead of completion order (fast, slow), with exactly one response per
call — the rollback path does not double-emit. Persisted order stays request-ordered either way,
which is not the flag's to change. Together with the existing
`streaming_tool_response_ordering.rs`, which gates the ON behaviour, the pair proves the flag
selects between two real behaviours rather than being dead config.

`BIOROUTER_TOOL_CALL_BATCHING=0` remains covered by its existing decoder unit test in
`providers/formats/anthropic.rs`; re-verified green this round.

Gate: `parallel_tool_batch_streaming_killswitch.rs`.

## PAR-07 — same-path writers never interleave {#par-07}

**PASS.** Two shell calls in one batch whose redirect targets are the same file, each writing 200
lines one at a time — the pattern that shreds a file without the lock. The result is 200 lines, all
from a single writer. Last-writer-wins is the expected and acceptable outcome; a *mixed* file would
mean the per-path exclusive locks failed. They did not.

Gate: `two_writers_to_the_same_path_never_interleave`.

## PAR-08 — the concurrency lever is start-up-only {#par-08}

**Low severity. Documented, not changed.**

`max_concurrent_tools()` reads the environment on every call, but the semaphore it feeds is a
`LazyLock` built on the **first** acquisition in the process. So `BIOROUTER_TOOL_MAX_CONCURRENT`
takes effect only if it is set before any tool dispatches — setting it on a running daemon does
nothing.

This is reasonable for a rollback lever (you set it and restart) and making it dynamic would mean
resizing a live semaphore for no clear gain. It is recorded because the variable's behaviour is not
self-evident from the code, and because anyone reaching for it in an incident will be reaching for it
on a daemon that is already running. The constraint is written into the gate's module comment so it
cannot be lost.

## Suites

Run after both fixes, from the `integrate/docs-cleanup` worktree under hermit.

| Suite | Result |
|---|---|
| `cargo test -p biorouter --no-fail-fast` | **1661 passed, 0 failed** (35 binaries) |
| `cargo test -p biorouter-mcp` | **809 + integration passed, 0 failed** |
| `cargo test -p biorouter-server --lib routes::apps` | **87 passed, 0 failed** |
| `cargo clippy -p biorouter -p biorouter-mcp --tests` | clean |
| `cargo fmt` | clean |

The seven new gates, by binary:

| Binary | Covers |
|---|---|
| `parallel_tool_batch_stress.rs` | PAR-01, PAR-03, PAR-07 |
| `parallel_tool_batch_cancellation.rs` | PAR-04 |
| `parallel_tool_batch_concurrency_cap.rs` | PAR-05 (ceiling) |
| `parallel_tool_batch_serialization_lever.rs` | PAR-05 (rollback lever) |
| `parallel_tool_batch_streaming_killswitch.rs` | PAR-06 |
| `parallel_batch_support/mod.rs` | shared harness |

They are split across binaries deliberately: `BIOROUTER_TOOL_MAX_CONCURRENT` and
`BIOROUTER_TOOL_RESPONSE_STREAMING` are process-global, and the semaphore behind the first is built
on first acquisition — so each override needs a process of its own, and a single combined binary
would race itself into flakiness.

## What was not driven

- **Live GUI.** These invariants are all at the loop/persistence seam and are observed far more
  precisely from the session store than from the DOM, which the campaign's own driver notes warn
  "lies across tab switches". The error-card *rendering* of a failed call was verified live earlier
  in this campaign (`dfa6dc32`); PAR-02 is what makes that rendering reachable for `shell`, and the
  `is_error` flag it now sets is the exact input that fix reads.
- **Cross-session contention.** The dispatch semaphore is global to the process, shared by every
  session. Two sessions each running a wide batch will contend for the same eight permits. Not
  exercised here, and worth a look if parallel-session use grows.
- **Subagent nesting.** The subagent tool is deliberately excluded from the semaphore (it would
  deadlock holding a permit while its leaf tools wait for one) and has its own. Verified by reading;
  the exclusion is keyed on the same bare tool name the dispatcher routes on, so the two cannot
  drift apart silently.

## Related documentation

- [The agent loop](../../agent-loop/README.md) — live documentation for the loop this round stresses.
- [Tool routing](../../agent-loop/tool-routing.md) — the living guidance on tool-result surfacing,
  which PAR-02 changes for `shell`.
- [QA round 1 results](qa-round-1-results.md) — where `isError` was first found being read wrongly
  by the frontend; PAR-02 is the backend half of the same bug.
- [Campaign final report](campaign-final-report.md) — the closing summary of the campaign.
