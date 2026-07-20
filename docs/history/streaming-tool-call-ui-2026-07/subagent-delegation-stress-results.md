# Subagent delegation: edge cases, stress, and chat traceability

> **What this is.** Round I2 of the campaign's stress sweep: the `subagent` tool driven through
> one delegation, many delegations, a failing child, a child that tries to spawn a child, a
> cancel mid-flight and a twelve-wide fan-out — with the four `SUB-NN` findings it raised and
> the gates that now hold them.
> **Status:** Historical record (completed 2026-07-20). The two fixes it describes are current;
> `SUB-03` and `SUB-04` were documented rather than changed.
> **Audience:** developers working on subagent delegation, the agent loop's abort path, and the
> desktop tool card.

Round I2 of the integration campaign, 2026-07-20, on `integrate/docs-cleanup`. The subject is the
`subagent` tool — the agent's one mechanism for handing a whole task to a child agent with its own
context — swept the way the previous round swept parallel tool batches: one delegation, many
delegations, a delegation that fails, a delegation cancelled mid-flight, and a fan-out wide enough
to hit the fork-bomb guards.

Findings are `SUB-NN`. Two were fixed, both revert-proven; two are documented and left alone with
the reasoning for why.

## What the round could and could not do

**Live GUI verification was not available.** Both configured providers (`versa_azure` and
`versa_bedrock`) are UCSF gateways behind an IP allowlist, and this machine's address —
`104.52.5.246`, no VPN — is not on it. Every completion returned `403 {"error":"The IP Address is
invalid"}`, for the desktop app and the CLI alike. There is no second provider configured and no
local model staged, so **no round-trip through a real LLM was possible**. The GUI was launched and
driven far enough to confirm the app boots clean and the renderer takes the frontend change with
zero console errors, and no further.

What replaced it is not a weaker substitute for one specific reason: **the subagent path can be
exercised end to end without a real model.** `dispatch_tool_call` hands the subagent tool the
parent's own `Arc<dyn Provider>`, so a child completes against the same provider object the parent
does. Scripting that one object therefore drives *both* halves of a real delegation through the real
agent loop, the real tool dispatcher, the real session store and the real result envelope. The
harness is [`crates/biorouter/tests/subagent_support/mod.rs`](../../../crates/biorouter/tests/subagent_support/mod.rs);
it tells parent from child by the system prompt (a child's is rendered from `subagent_system.md`,
which always opens "You are a specialized subagent") and reads a `TASK:<script>` marker out of the
child's instructions to decide how that particular child behaves — answer, stall, fail, stay silent,
or try to spawn a child of its own.

The one thing this cannot verify is what a real model *chooses* to do with the tool. Everything
structural — attribution, error propagation, teardown, the concurrency ceiling — it verifies
better than a GUI run would, because the assertions can read the persisted session rather than
the DOM.

## Findings

### SUB-01 (HIGH, fixed) — every subagent card in the transcript said the same thing

The headline requirement of this round was that parallel subagents be individually distinguishable
in the chat. They were not.

The desktop transcript has no subagent-specific rendering at all — `grep -rn subagent ui/desktop/src`
matches exactly one file, the generated API types. A delegation is drawn by the generic tool card,
whose header comes from `summarizeToolCall` in `ToolCallWithResponse.tsx`. That function is a chain
of name-matching special cases (`text_editor`, `shell`, `apply_patch`, anything containing `search`,
…) ending in a fallback that labels a call by its *argument keys*:

```ts
return `${displayName} with ${keys.slice(0, 3).join(', ')}${keys.length > 3 ? '…' : ''}`;
```

`subagent` matched no special case, and every subagent call carries the same keys. Measured before
the fix:

| call | rendered header |
|---|---|
| `{instructions: "Count the .rs files under crates/…"}` | `Subagent with instructions` |
| `{instructions: "Read Cargo.toml and report the workspace version."}` | `Subagent with instructions` |
| `{instructions: "List the crates in the workspace."}` | `Subagent with instructions` |
| `{subworkflow: "lint", parameters: {…}}` | `Subagent with subworkflow, parameters` |

Tool cards render with `isStartExpanded={false}`. A turn that fanned out to five subagents was
therefore five collapsed rows reading `Ran Subagent with instructions · 1 result ready`, in
completion order rather than call order — identical, unordered, and giving the user no way to tell
which result came from which child short of expanding all five. A `subworkflow` call did not even
name the subworkflow it ran.

This is the traceability failure the round was looking for, and it is worse for subagents than for
any other tool, because a delegation is precisely the case where the interesting content is *not*
the call but which of several concurrent workstreams it belongs to.

Fixed with a `subagent` branch placed ahead of the name-matching chain, which labels a delegation by
its task: `Delegating: Count the .rs files under crates/.`, or `Delegating lint: Only check the new
files.` when a subworkflow is named. Long instructions are elided to their opening sentence rather
than dropped.

Gated at two levels in `ToolCallWithResponse.test.tsx` — the label function (three different
instructions must produce three different labels; the argument-key fallback must never appear) and
a jsdom render of two collapsed delegation cards asserting each shows its own task. Revert-proven:
deleting the one dispatch line fails five assertions.

### SUB-02 (HIGH, fixed) — a subagent that never ran reported `completed`

A child whose turn aborts — provider auth failure, rate limit, tool loop, worker timeout — came back
to the parent as a **success**:

```text
is_error = false
status   = "completed"
summary  = "Ran into this error: Execution error: … Please retry if you think this is a
            transient or recoverable error. Biorouter already retried it once."
```

The mechanism is a shape collision. When a turn aborts, the agent loop writes a human-readable
explanation into the conversation as an ordinary assistant text message and yields
`AgentEvent::TurnAborted`. `run_complete_subagent_task` consumed that event, logged it, and
`break`; the abort itself was then dropped on the floor. `SubagentResult::from_conversation` was
handed the conversation, saw a last assistant message with non-empty text — which is its definition
of a completed run — and graded it `Completed` with `is_error: false`.

The consequences run the whole way up. The parent's tool card renders green. The structured envelope
tells any programmatic consumer the delegation succeeded. And the parent model is handed a summary
that opens "Ran into this error" while the envelope beside it says `completed`, which is exactly the
kind of contradiction a model resolves by trusting the structured field.

This is the same defect class as `PAR-02` from the previous round (`shell` discarded its exit
status) and `R1`'s `isError` bug: an operation that failed is byte-indistinguishable at the protocol
level from one that succeeded, so every downstream renderer that was fixed to show failures in red
had nothing to show.

Fixed by carrying the abort out of the child's event stream — `get_agent_messages` now returns
`Option<(wire_code, message)>` alongside the conversation — and by adding
`SubagentResult::from_aborted_turn`, which grades the run `Error`, names the wire code and the cause,
and preserves the child's artifacts plus its last substantive message when that message is not
simply the abort notice repeated. A stream error is treated the same way rather than silently
truncating the run.

Revert-proven: restoring `from_conversation` on the abort path fails the gate with the exact
before-state above.

### SUB-03 (MEDIUM, documented) — a turn-capped child also reports `completed`

A child that only ever calls tools and never writes a summary runs out of turns. What comes back is
the agent loop's own stop notice:

> I've reached my action limit for this turn (3 actions without user input), so I'm stopping here
> rather than because the task is necessarily complete. Would you like me to continue? (raise the cap
> with `max_turns` / `BIOROUTER_MAX_TURNS`.)

…graded `completed`, `is_error: false`.

The prose is honest — it says outright that it stopped for the cap, and the parent model receives it
verbatim, which is enough for the model to react. The *status* is the part that overstates, and the
question the notice asks ("Would you like me to continue?") is addressed to a user who does not exist
in a subagent that cannot be continued.

Left unfixed deliberately. Unlike SUB-02 there is no structural signal to key off: a turn-limit stop
is emitted as a plain `AgentEvent::Message`, distinguishable from a real summary only by matching the
notice's own English. Correcting it properly means giving the loop a machine-readable turn-limit
signal, which changes the event surface for the main agent too — CLI exit codes and GUI rendering
included — and is not a change this round should make unilaterally. The behaviour is pinned as-is in
the failure-modes gate with a comment explaining why, so the day the loop grows that signal, that
assertion is the thing that fails and points at the envelope.

### SUB-04 (LOW, documented) — `SubagentStatus::Incomplete` is nearly unreachable in practice

`Incomplete` exists for a child that "ran but produced no final text". Reaching it requires the
child's conversation to end on a tool call. In a live run it essentially cannot: every exit path out
of the agent loop — turn cap, tool-call cap, tool loop, abort — writes an assistant text message on
the way out, so `from_conversation` always finds text and grades `Completed`. The status is
well-covered by unit tests and effectively dead on the live path. Not a bug; worth knowing before
anyone reasons about `incomplete` as a signal they can rely on.

## What passed, and is now gated

Seven gates in `crates/biorouter/tests/subagent_delegation.rs`, plus one in
`crates/biorouter/tests/subagent_cancellation.rs`. The cancellation case has a binary of its own for
the same reason the parallel-batch kill switches did: the in-flight subagent counter and the
concurrency semaphore are process-global, so "nothing leaked" is only a truthful assertion when
nothing else is running.

| Gate | What it holds |
|---|---|
| `a_single_subagent_runs_and_returns_its_own_summary` | One delegation runs the child, and the child's own summary comes back on the call that asked for it, graded `completed`. |
| `parallel_subagents_keep_their_results_separate` | Three different children in one batch: every call answered, each response carries its own child's summary, and **no response contains a sibling's** — the backend half of the traceability requirement. |
| `a_subagent_cannot_spawn_a_subagent_and_fails_cleanly` | The depth limit is 1. A child's `subagent` call is refused at dispatch, the refusal reaches it as an ordinary tool error, it reports the refusal in its summary, and the grandchild's script never starts. The parent's call does **not** fail. |
| `failing_silent_and_slow_children_all_surface` | A failing, a silent and a slow child in one batch. Three responses, nothing swallowed; SUB-02's `is_error` + named cause; SUB-03 pinned; never the old "No text content in last message" placeholder. |
| `cancelling_the_parent_tears_down_its_children` | Cancelling mid-flight returns in well under the children's remaining runtime, every in-flight slot is released, and **no `tool_use` block is persisted without its `tool_result`** — `PAR-04`'s invariant, held for delegation too. |
| `a_wide_batch_of_subagents_loses_nothing` | Twelve at once: twelve results, none crossed over, every child actually ran, and peak concurrency stays at or under the fork-bomb guard's ceiling of 8. |
| `subagents_and_ordinary_tools_share_a_batch` | Delegation and ordinary parallel tool calls in the same batch. Both dispatch paths coexist and every result still lands on its own call. |
| `steering_mid_delegation_costs_no_subagent_results` | A soft interrupt queued while children are in flight: both delegations still report, the steer lands in the same turn, and nothing is left pending. |

Two behaviours worth stating explicitly because they were checked rather than assumed:

- **Depth is capped at one, twice over.** `subagents_enabled` refuses to *list* the tool for a
  session of type `SubAgent`, and `dispatch_tool_call` refuses to *run* it for one. Belt and braces:
  a child that hallucinates the tool name still gets a clean `INVALID_REQUEST` rather than recursion.
- **Cancellation reaches the child.** The blocking path passes the parent turn's cancellation token
  straight through to the child's `agent.reply`. (The *background* path deliberately does not — a
  detached subagent is meant to outlive the turn that started it — but background subagents are off
  by default, gated behind `BIOROUTER_SUBAGENT_BACKGROUND`, so no live session takes that path.)

## Files changed

| Path | Why |
|---|---|
| `crates/biorouter/src/agents/subagent_handler.rs` | Carry the child's turn abort out of the event stream instead of discarding it (SUB-02). |
| `crates/biorouter/src/agents/subagent_result.rs` | `from_aborted_turn`: grade an aborted run `error`, name the cause, salvage the work (SUB-02). |
| `ui/desktop/src/components/ToolCallWithResponse.tsx` | Label a delegation by its task, not by its argument keys (SUB-01). |
| `ui/desktop/src/components/ToolCallWithResponse.test.tsx` | The SUB-01 gate, at label level and at render level. |
| `crates/biorouter/tests/subagent_support/mod.rs` | The scripted-provider harness both test binaries share. |
| `crates/biorouter/tests/subagent_delegation.rs` | Seven gates. |
| `crates/biorouter/tests/subagent_cancellation.rs` | The teardown gate, isolated for the process-global counters. |

## Reproducing

```bash
source bin/activate-hermit
export CARGO_TARGET_DIR=/tmp/br-integrate-target
cargo test -p biorouter --test subagent_delegation --test subagent_cancellation
cargo test -p biorouter --lib agents::subagent
cd ui/desktop && npx vitest run src/components/ToolCallWithResponse.test.tsx
```

## Related documentation

- [Streaming tool-call UI campaign](README.md) — the campaign index this round belongs to.
- [Parallel tool-batch stress results](parallel-tool-batch-stress-results.md) — the preceding round, swept the same way; `PAR-04`'s no-orphan invariant is re-checked here for delegation.
- [Campaign final report](campaign-final-report.md) — the closing summary, where round I2's defects are recorded against the branch's merge decision.
- [Master checklist](master-checklist.md) — item `I2`, the instruction this round answers.
