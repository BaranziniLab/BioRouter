# Parallel stress test — issue sweep 2026-07

> **What this is.** The record of the post-sweep concurrency stress test: parallel
> headless `biorouter run` fleets on both UCSF Versa providers, deliberately
> maximizing shared-session-store read/write contention with the desktop GUI's
> daemon open on the same database — the live proof for the #31/#41/#40 fixes.
> **Status:** Historical record — executed 2026-07-26 late evening, on main after
> all nine sweep batches merged (through `dd545745`).

## Setup

- CLI: `target/debug/biorouter` built from final main, re-signed with the UCSF
  Developer ID so Keychain provider keys resolve without prompts.
- Models (per campaign policy, Versa only): `versa_azure` `gpt-5.5-2026-04-24`
  and `versa_bedrock` `us.anthropic.claude-opus-4-8`.
- Config: sandboxed `XDG_CONFIG_HOME` copy of the real config with the
  `developer` builtin enabled (the real config keeps it off for the user's
  benchmarks); `BIOROUTER_MODE=auto`. Data home real — every named-session run
  writes the shared `~/.local/share/biorouter/sessions/sessions.db` (~208 MB,
  1,871 sessions / 39k messages at start) **while the dev GUI's `biorouterd`
  was live on the same store** — the exact CLI+GUI contention scenario from #31.
- Tasks: five deterministic tool-heavy jobs (shell + text_editor: fibonacci sum,
  line/word counts, CSV stats, directory trees, JSON round-trip), each in a
  private sandbox dir, each ending with an exact machine-checkable marker so
  "comprehensive results" is asserted, not eyeballed. Harness:
  [`stress-harness.py`](stress-harness.py) (archived copy).

## Pre-fleet live smokes

| Check | Result |
|---|---|
| `versa_azure` gpt-5.5 one-turn round-trip | PASS (valid JSON, exact marker) |
| `versa_bedrock` `us.anthropic.claude-opus-4-8-v1` | **REJECTED** — `ValidationException: The provided model identifier is invalid` (the review's skepticism was right; the sweep's original #29 entry used this spelling) |
| `versa_bedrock` `us.anthropic.claude-opus-4-8` | PASS (real assistant reply) → shipped id fixed on main in `dd545745` |

The failed `-v1` smoke also exercised the new failure machinery end-to-end: one
retry, the error recorded in a **valid** `--output-format json` document, exit
code 70 with `provider_failure` — no hang, no stdout corruption (#40/#31 wire
behavior confirmed against a real provider error).

## Fleet results

| Round | Runs | Concurrency | Session mode | rc=0 | valid JSON | answers exact | UNIQUE violations | `not connected` | timeouts | wall |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | 16 | 8 | mixed (named-shared + `--no-session`) | 16/16 | 16/16 | 16/16 | 0 | 0 | 0 | 52 s |
| 2 | 36 | 12 | **all shared DB** | 36/36 | 36/36 | 36/36 | 0 | 0 | 0 | 84 s |

Post-fleet audit of the shared store (52 new agent sessions written under
contention, GUI open throughout): `PRAGMA integrity_check` = ok; 1,907 sessions /
39,234 messages; **duplicate `(session_id, msg_uid)` pairs: 0**.

## Conclusions

- The #41 Bedrock decoder batching + uid adoption + store retry eliminated the
  `UNIQUE constraint failed: messages.session_id, msg_uid` class entirely at
  3× the concurrency where the issue reported ~44–50% failure.
- `--no-session` runs (round 1) left the shared store untouched by construction
  (#31 isolation) and cleaned up their private stores.
- No permission-gate hangs and no `Error: not connected` in 52 tool-heavy auto-mode
  runs (#40), and every run — including the deliberately failed smoke — emitted
  exactly one valid JSON document.
- Cross-session tool calls did not interfere: all 52 sandboxes contained exactly
  the expected artifacts and all end-marker answers were exact.

No new defects surfaced; nothing required fixing out of this phase beyond the
`-v1` model-id correction recorded above.
