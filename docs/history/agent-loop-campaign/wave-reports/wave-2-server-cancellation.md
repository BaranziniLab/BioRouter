# Wave 2 — Server & cancel cluster

Branch `agent-loop-server`, off integration `be342632`.

**Gate: GREEN.** Verified by the orchestrator directly (the cluster's own two
verifier agents were both cut off — the first by a full disk, the second
mid-run — leaving these proposals unverified; this run closes that gap).

## Proposals

| BR | Title | Commit |
|---|---|---|
| BR-6 | Token-aware large-response handling (head/tail preview + in-workspace handle) | `5e70ecd6` |
| BR-7 | Externalize large tool results from `content_json` (`message_blobs` side table) | `9b820410` |
| BR-52 | Carry the agent-computed `TokenState` in the event stream (kill per-token DB reads) | `104802c8` |
| BR-61 | Wire the orphaned `/interrupt` soft-interrupt to the desktop client | `031fbc98` |
| BR-62 | Reliable cancel — addressable `/agent/cancel`, request-scoped confirmations with TTL, cancellation-aware waits, idempotent `/reply` | `95464750` |

## Test evidence

Full 5-crate regression, `CARGO_INCREMENTAL=0`, run to completion:

```
47 suites ok, 1 suite FAILED
2067 tests passed   (GATE-1 baseline: 2024 → +43 new tests from this cluster)
Only failure: test_anthropic_provider  — pre-existing, live Anthropic API, red in the baseline too
```

`cargo fmt --all -- --check`: clean. Working tree: clean.

Note: `biorouter-server tunnel::lapstone_test` failed during an earlier attempt with
HTTP 503 — it hits a live third-party Cloudflare worker. It **passed** in this run,
confirming it was environmental, not a regression. This cluster touches no tunnel code.

## Design decisions worth reviewing

- **BR-62 inherited a broken skeleton.** A previous agent was killed mid-task and left
  an uncommitted BR-62 draft whose design was sound but which called six helpers that
  did not exist — it did not compile. It was assessed and finished rather than
  discarded or blindly trusted.
- **Request-scoped confirmations.** The old single per-agent mpsc meant a stale or
  duplicate POST could resolve *a different* pending tool request. Now one oneshot per
  prompt, keyed by tool-request id, registered *before* the action-required card is
  yielded (so an instant client answer cannot arrive before a sender exists).
- **Cancel/expiry always write a tool response.** A tool request with no response
  breaks the next provider call, so cancelling answers every remaining prompt — a
  cancelled turn leaves a well-formed conversation.
- **New default behaviour, config-gated:** permission-prompt TTL
  `BIOROUTER_CONFIRMATION_TIMEOUT_SECS`, default 3600s (deliberately generous — a
  premature expiry is indistinguishable from a denial the user never made). `0`
  restores the pre-BR-62 wait-forever behaviour (still cancellable).
- **`/agent/cancel` returns 200 `{cancelled:false}` for an idle session**, not 409 — a
  Stop button that errors because the turn already finished is exactly the
  unreliability this BR removes. (Deliberately different from BR-61's `/interrupt`,
  which *does* 409 with no turn in flight: a steer with no running loop would be
  stranded.)
- **OpenAPI regenerated in-commit.** Routes changed (`POST /agent/cancel`; `/reply`
  gains optional `turn_id`; `/action-required` gains `status`). `openapi.json` +
  generated TS client are committed.

## Known follow-ups (NOT done here)

1. **The desktop UI is not wired to the new surface.** The generated client exposes
   `cancelTurn()` and `ChatRequest.turn_id`, but `ui/desktop/src/hooks/chatStreamStore.tsx`
   neither sends a `turn_id` nor calls `cancelTurn()` on Stop. The server side is
   complete and independently correct, but **the SSE-reconnect dedupe and the new cancel
   endpoint are inert for GUI users** until that wiring lands. Scheduled for Wave 3.
2. **Cancellation is cooperative and boundary-only.** A long-running in-process tool
   body that ignores the cancel token still cannot be force-aborted (loop-detection
   gap #7). Out of scope here; would touch built-in tool bodies in `biorouter-mcp`.
