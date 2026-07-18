# Wave 2 — server and cancel cluster verification report

> **What this is.** Gate evidence for the Wave 2 server cluster — BR-6 large-response handling,
> BR-7 externalized blobs, BR-52 token state in the event stream, BR-61 interrupt wiring and
> BR-62 reliable cancel — plus the design decisions behind request-scoped confirmations and the
> follow-ups it deliberately left undone.
> **Status:** Historical record — this cluster cleared the gate and merged into the campaign's
> `agent-loop-integration` branch at Wave 2. `session/message_blobs.rs` exists in the tree
> today, and the one open follow-up named below — wiring the desktop UI to `cancelTurn()` and
> `turn_id` — **was completed as BR-62b in Wave 3**; see
> [the Wave 3 polish report](wave-3-polish.md). The verification run itself is undated in the
> original record.
> **Audience:** maintainers auditing what the campaign shipped and on what evidence.

The agent-loop fix campaign implemented 67 numbered proposals (`BR-1` … `BR-67`) from
[the master improvement-proposals list](../../agent-loop-review/improvement-proposals.md).
Related proposals were grouped into **clusters**, each built in its own git worktree, and
clusters shipped in dependency-ordered **waves**. Every wave had to clear a **gate**: a full
per-crate test run admitting zero new failures against a recorded baseline. This file is the
server cluster's gate evidence, for branch `agent-loop-server`, cut from integration commit
`be342632`. Campaign conventions and the wave table are in
[the campaign overview](../README.md).

> **Warning.** This report is thinner than its sibling wave reports and carries **no per-crate
> evidence table** — only a five-crate summary. That is a limitation of how the run happened,
> not of the cluster's scope: the cluster's own two verifier agents were both cut off, the
> first by a full disk and the second mid-run, leaving these proposals unverified. The
> orchestrator then verified them directly, and this run closes that gap. Read the totals below
> as a whole-run summary rather than as per-crate proof.

## Proposals shipped

Each proposal's full problem statement is in
[the master improvement-proposals list](../../agent-loop-review/improvement-proposals.md).

| BR | Title | Commit |
|---|---|---|
| BR-6 | Token-aware large-response handling (head/tail preview + in-workspace handle) | `5e70ecd6` |
| BR-7 | Externalize large tool results from `content_json` (`message_blobs` side table) | `9b820410` |
| BR-52 | Carry the agent-computed `TokenState` in the event stream (kill per-token DB reads) | `104802c8` |
| BR-61 | Wire the orphaned `/interrupt` soft-interrupt to the desktop client | `031fbc98` |
| BR-62 | Reliable cancel — addressable `/agent/cancel`, request-scoped confirmations with TTL, cancellation-aware waits, idempotent `/reply` | `95464750` |

## Test evidence

Full 5-crate regression, `CARGO_INCREMENTAL=0`, run to completion:

```text
47 suites ok, 1 suite FAILED
2067 tests passed   (GATE-1 baseline: 2024 → +43 new tests from this cluster)
Only failure: test_anthropic_provider  — pre-existing, live Anthropic API, red in the baseline too
```

`cargo fmt --all -- --check`: clean. Working tree: clean.

`biorouter-server tunnel::lapstone_test` failed during an earlier attempt with HTTP 503 — it
hits a live third-party Cloudflare worker. It **passed** in this run, confirming it was
environmental, not a regression. This cluster touches no tunnel code.

**Gate: GREEN**, on the evidence above and subject to the completeness caveat at the top of this
report.

## Design decisions worth reviewing

- **BR-62 inherited a broken skeleton.** A previous agent was killed mid-task and left an
  uncommitted BR-62 draft whose design was sound but which called six helpers that did not
  exist — it did not compile. It was assessed and finished rather than discarded or blindly
  trusted.
- **Request-scoped confirmations.** The old single per-agent mpsc meant a stale or duplicate
  POST could resolve *a different* pending tool request. Now there is one oneshot per prompt,
  keyed by tool-request id, registered *before* the action-required card is yielded, so an
  instant client answer cannot arrive before a sender exists.
- **Cancel and expiry always write a tool response.** A tool request with no response breaks the
  next provider call, so cancelling answers every remaining prompt — a cancelled turn leaves a
  well-formed conversation.
- **New default behaviour, config-gated:** permission-prompt TTL
  `BIOROUTER_CONFIRMATION_TIMEOUT_SECS`, default 3600 s, deliberately generous — a premature
  expiry is indistinguishable from a denial the user never made. `0` restores the pre-BR-62
  wait-forever behaviour, which is still cancellable.
- **`/agent/cancel` returns 200 `{cancelled:false}` for an idle session**, not 409 — a Stop
  button that errors because the turn already finished is exactly the unreliability this
  proposal removes. This is deliberately different from BR-61's `/interrupt`, which *does* 409
  with no turn in flight: a steer with no running loop would be stranded.
- **OpenAPI regenerated in-commit.** Routes changed: `POST /agent/cancel`; `/reply` gains
  optional `turn_id`; `/action-required` gains `status`. `openapi.json` and the generated TS
  client are committed.

## Follow-ups this cluster did not do

1. **The desktop UI is not wired to the new surface.** The generated client exposes
   `cancelTurn()` and `ChatRequest.turn_id`, but `ui/desktop/src/hooks/chatStreamStore.tsx`
   neither sends a `turn_id` nor calls `cancelTurn()` on Stop. The server side is complete and
   independently correct, but **the SSE-reconnect dedupe and the new cancel endpoint are inert
   for GUI users** until that wiring lands. Scheduled for Wave 3.

   > **Note.** This was done. BR-62b landed the wiring in Wave 3 (commit `3225ce5b`, with 3 new
   > `chatStreamStore` unit tests); see [the Wave 3 polish report](wave-3-polish.md).

2. **Cancellation is cooperative and boundary-only.** A long-running in-process tool body that
   ignores the cancel token still cannot be force-aborted. This is gap #7 in the pre-campaign
   [loop and stuck detection review](../../agent-loop-review/subsystem-reviews/loop-and-stuck-detection.md),
   whose "Gaps & weaknesses" list is where the campaign's gap numbering comes from. Out of scope
   here; a fix would touch built-in tool bodies in `biorouter-mcp`. It remained open at the end
   of the campaign — see [the campaign outcome report](../outcome-report.md).

## Related documentation

- [Agent-loop fix campaign overview](../README.md) — the wave table, cluster conventions and
  merge status this report is evidence for.
- [Master improvement proposals](../../agent-loop-review/improvement-proposals.md) — the
  definition of BR-6, BR-7, BR-52, BR-61 and BR-62.
- [Wave 3 — polish cluster](wave-3-polish.md) — where BR-62b closed this report's first
  follow-up by wiring the desktop Stop button.
- [Loop and stuck detection review](../../agent-loop-review/subsystem-reviews/loop-and-stuck-detection.md)
  — the source of the gap numbering, including the cooperative-cancellation gap #7.
- [Campaign outcome report](../outcome-report.md) — the end-of-campaign totals and the list of
  what stayed open.
