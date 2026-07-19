# Tool-call UI latency — implementation status (2026-07-18)

Integration verification of the work implemented against
[`docs/investigations/2026-07-18-tool-call-ui-latency.md`](../investigations/2026-07-18-tool-call-ui-latency.md).
Measurement register: [`2026-07-18-baseline.md`](2026-07-18-baseline.md).

This document records what landed, what is **verified**, and what is **asserted
but unmeasured**. Where the two differ, the difference is stated rather than
smoothed over.

---

## 0. Branch identity — read this first

The task that produced this document named the branch
`investigate/tool-call-ui-latency`. **That is not where this work lives.**

| | |
|---|---|
| Branch actually holding the work | `feat/boot-splash-mark-cascade` (HEAD `bebfb664`) |
| `investigate/tool-call-ui-latency` | `a24f74b7` — **dashboard-mode removal**, a different track, now merged into `main` |
| Merge base with `main` | `b9a37d72` |
| `main` tip | `8fc104d9` (advanced by 12 commits: the dashboard-removal merge) |

Two consequences:

1. **`git diff main..HEAD` is misleading on this branch** and must not be used to
   review it. Because `main` moved, that diff re-reports the entire dashboard
   removal as if this branch had authored it — ~40 `Dashboard/` files and a
   `ui/desktop/src/styles/main.css` hunk that **this branch never touched**
   (`git log main..HEAD -- ui/desktop/src/styles/main.css` is empty). Use
   `git diff $(git merge-base main HEAD)..HEAD` — 27 files, not 95.
2. **This branch is not rebased onto current `main`.** The latency work has never
   been compiled or tested against the post-dashboard-removal tree. Every gate
   below passed on the *pre-removal* base. A rebase-and-re-run is required before
   merge; conflicts are likely in `agent.rs` / `BaseChat.tsx`, which both tracks
   touched.

Also: this branch mixes two unrelated tracks — the boot splash (`84466ec7`,
`7a0d39f0`) and the latency work. They should be separated before review.

---

## 1. What landed

Nine commits from the merge base, plus one integration fix.

| Commit | Item | What it does |
|---|---|---|
| `27f88710` | §6.0 Stage 0 | Phase instrumentation: new `agents/phase_timing.rs`, `BIOROUTER_PHASE_TIMING=1` read once into a `LazyLock<bool>`. Emits `tracing::debug!(target: "phase")` spans around `integrate_tool_result`, `assemble_turn_context`, `SecretGuard::for_dir`, and `session_manager::add_message`. **Off by default; no behaviour change when unset.** |
| `e0b5928c` | §6.1c | Anthropic streaming path now surfaces `thinking` blocks, which the non-streaming path already did. Includes `test_streamed_thinking_replays_into_next_request` — thinking blocks must replay into the next request or the model loses its own reasoning. |
| `7385be3c` | Bedrock streaming | `bedrock` + `versa_bedrock` stream via `ConverseStream` instead of blocking `Converse`, so tool cards appear during generation. Large: ~1105 lines added to `providers/formats/bedrock.rs`, 52 test fns in that file. |
| `032c938c` | versa_azure/azure streaming | Implements `stream()` for both. Previously `complete()`-only, so the whole turn landed at once. |
| `9c96dd77` | wave-1 review remediation | Fixes across `agent.rs`, `phase_timing.rs`, `azure.rs`, `versa_azure.rs`, `formats/anthropic.rs`, plus new `providers/utils.rs` helpers. Created the baseline register. |
| `2ef3a56b` | §6.2a / H6 | **The one measured win.** See §3. |
| `bebfb664` | integration fix | Mine. Removed a dead `use tokio::sync::Mutex` in `biorouter-cli/src/scenario_tests/scenario_runner.rs` left behind by `2ef3a56b`. See §5. |
| `84466ec7`, `7a0d39f0` | — | Boot splash. **Unrelated to latency**; listed only because they share the branch. |

---

## 2. Forbidden-path audit

Rule: this branch must not modify the concurrent theme session's files.

**Result: one violation, low harm.**

`docs/design/boot-splash-studio.html` was **added** by `84466ec7`, and
`docs/design/*` is on the protected list. It is a new file (`A` in
`--name-status`, absent at both the merge base and on `main`), so it collides
with the *rule* but not with anyone's work — no concurrent-session content was
overwritten. Flagging it because the rule is the rule; it is a design-studio
scratch file for the boot splash and is arguably misfiled regardless.

Every other protected path is clean in our commits:
`styles/main.css`, `styles/codeTheme.ts`, `styles/codeTheme.test.ts`,
`contexts/ThemeContext.tsx`, `BioRouterSidebar/*`, `InAppTerminalDock.tsx`,
`check-contrast.mjs` — **none touched**. The `main.css` hunk visible in
`git diff main..HEAD` is the `main`-moved artifact described in §0, not ours.

**Concurrent session's uncommitted work: intact.** Verified before and after my
commit — 9 modified + 4 untracked files, unchanged throughout:

```
 M docs/design/alma-mater-theme.md
 M ui/desktop/scripts/check-contrast.mjs
 M ui/desktop/src/components/BioRouterSidebar/AppSidebar.tsx
 M ui/desktop/src/components/BioRouterSidebar/ThemeFamilySelector.tsx
 M ui/desktop/src/components/InAppTerminalDock.tsx
 M ui/desktop/src/contexts/ThemeContext.tsx
 M ui/desktop/src/styles/codeTheme.test.ts
 M ui/desktop/src/styles/codeTheme.ts
 M ui/desktop/src/styles/main.css
?? docs/design/alma-mater-light-redesign.html
?? docs/design/roche-limit-theme.html
?? docs/design/roche-limit-theme.md
?? ui/desktop/src/components/BioRouterSidebar/ThemeFamilySelector.test.tsx
```

---

## 3. The MCP mutex removal (§6.2a / H6) — the one measured result

`McpClientBox` was `Arc<Mutex<Box<dyn McpClientTrait>>>` and `dispatch_tool_call`
held the guard across the entire `call_tool` await. Since `call_tool` takes
`&self` and implementations are internally synchronized, the outer mutex bought
nothing and turned `max(tool durations)` into `sum(tool durations)` for
concurrent calls on one extension.

`McpClientBox` is now `Arc<dyn McpClientTrait>`; the guard is dropped at all 9
`client.lock().await` sites, including one held across the paginated `list_tools`
loop.

### Before / after

| | before | after |
|---|---|---|
| 3 concurrent 400 ms dispatches, one extension | **1.208 s** (≈ sum) | **0.42 s** (≈ max) |

**Verification status: after-number independently confirmed, before-number not.**
I re-ran the gate and observed `finished in 0.48s` wall — consistent with ~max
(400 ms), not sum (1200 ms). The 1.208 s "before" is from the commit message; I
did not revert to re-measure it. The test itself is a real regression gate rather
than a tautology — it asserts `elapsed < 700ms` against a 1200 ms serialized
worst case, so it genuinely fails on the pre-fix code:

```rust
assert!(
    elapsed < std::time::Duration::from_millis(700),
    "3 concurrent calls on one extension took {elapsed:?}; expected ~400ms. \
     They are being serialized on a client-wide mutex (H6)."
);
```

Gate: `cargo test -p biorouter --lib h6_parallel_same_extension` — **passes**.

### Concurrency safety note (carried forward for human review)

Removing the mutex withdrew free mutual exclusion from five in-process trait
impls. Four hold only immutable/shared state. **`todo_extension` did not** —
`with_state` is a read-modify-write across two await points that rewrites the
whole `extension_data` blob, so two concurrent `todo_add`s could lose an update.
`2ef3a56b` gave it a narrow `state_lock`. Per `HOWTOAI.md`, this class of change
(MCP protocol + async concurrency) **requires human review regardless** — the
audit is asserted in the commit message and I did not independently re-derive it
for all five impls.

Rollback lever: `BIOROUTER_TOOL_MAX_CONCURRENT=1` still serializes tool
execution, wrapping the dispatch future in `agent.rs` strictly outside the
removed mutex.

---

## 4. Gate results

All run at `bebfb664` on the pre-rebase base (see §0 caveat).

| Gate | Result |
|---|---|
| `cargo check --workspace` | **PASS** — `Finished dev profile in 1m 59s` |
| `cargo test -p biorouter --lib` | **PASS** — `1446 passed; 0 failed; 0 ignored` |
| `cargo test -p biorouter-mcp --lib` | **PASS** — `800 passed; 0 failed; 2 ignored` |
| `cargo test --test mcp_integration_test` | **PASS** — `4 passed; 0 failed` |
| `cd ui/desktop && npm run test:run` | **PASS** — `158 test files, 1342 passed` |
| `cd ui/desktop && npm run lint:check` | **PASS** — exit 0; typecheck + eslint `--max-warnings 0` + `228 contrast assertions pass` |
| `cargo fmt --check` | **PASS** — exit 0, no output |
| `./scripts/clippy-lint.sh` | **PASS after fix** — exit 101 before, exit 0 after `bebfb664` |

---

## 5. The failure I found and fixed

`./scripts/clippy-lint.sh` failed with exit 101:

```
error: unused import: `tokio::sync::Mutex`
   --> crates/biorouter-cli/src/scenario_tests/scenario_runner.rs:145:9
    |
145 |     use tokio::sync::Mutex;
    |         ^^^^^^^^^^^^^^^^^^
    |
    = note: `-D unused-imports` implied by `-D warnings`
error: could not compile `biorouter-cli` (lib test) due to 1 previous error
```

**Diagnosis.** `2ef3a56b` changed the mock client construction from
`Arc::new(Mutex::new(Box::new(mock_client)))` to `Arc::new(mock_client)`,
removing the only `Mutex` use in the file (`grep -n Mutex` confirmed line 145 was
the sole remaining reference) but leaving the function-local `use`.

**Why every other gate missed it.** The import sits inside
`run_provider_scenario_with_validation`, which is only built for the lib-test
target. `cargo check --workspace` does not build test targets;
`clippy-lint.sh` builds `--all-targets` with `-D warnings`. This is a gap worth
knowing about: **`cargo check --workspace` passing is not evidence that
clippy will pass.**

Fix: deleted the line. One deletion, no behaviour change. Committed as
`bebfb664`, staged by explicit path.

---

## 6. Verified vs. unverified — the honest split

### Verified by test

- The H6 mutex removal, with a real timing gate (§3).
- Anthropic streamed `thinking` blocks, incl. replay into the next request.
- Phase-timing plumbing compiles, is off by default, costs one `LazyLock` read.
- Azure / versa_azure `stream()` **payload shape**: `stream: true`,
  `stream_options.include_usage: true`, correct Azure deployment path,
  `supports_streaming()` true.
- Bedrock `ConverseStream` decoding, against 52 unit tests in `formats/bedrock.rs`.

### NOT verified — needs a live smoke test

**`versa_azure` streaming against the UCSF Versa endpoint is the big one.**
It requires `VERSA_AZURE_API_KEY` + UCSF network access, neither available here.

The register's own words, which I confirm and endorse:

> If the UCSF Versa proxy buffers SSE and releases the response in one piece, the
> user's perceived latency is *unchanged* while the code path, the commit
> message, and the test suite all assert an improvement. Nothing in the current
> test suite can distinguish that outcome from success.

So: **the headline latency claim of this branch is unmeasured.** The shape tests
prove we send a streaming request; they cannot prove we receive a streaming
response. Procedure — configure `versa_azure`/`gpt-5.5-2026-04-24`, run one turn
provoking a tool call under
`BIOROUTER_PHASE_TIMING=1 RUST_LOG=phase=debug`, record
`WAITING_LLM_STREAM_OPENED.open_ms` vs `WAITING_LLM_STREAM_EXHAUSTED.total_ms`.
**If `open_ms` is within noise of `total_ms`, the proxy is buffering and the
change delivers nothing — record that here rather than closing the item.**

Also unverified live: `azure` streaming (same change, no endpoint), Bedrock
`ConverseStream` against real AWS (unit-tested only), and any end-to-end
perceived-latency number in the GUI. **No GUI smoke test was run at all.**

By the report's own §6.5 Invariant 6 — *"A fix without a measurement is not
landed"* — the versa_azure, azure, and Bedrock streaming items are **not landed**,
however green the suite. Only §6.2a carries a number.

---

## 7. Unimplemented from the plan

| Item | Priority (report) | Status |
|---|---|---|
| **§6.1b** pending tool-call events (Anthropic + OpenAI) | **highest (perceived)** | **not started** |
| **§6.2b** batch `tool_use` blocks | **high (real)** | **not started** |
| **§6.2c** per-tool response emission | med (real) | **not started** |
| **§6.2d** `SecretGuard` per-cwd cache | low | **not started** |

Sequencing constraints that survive into whatever lands next:

- **§6.2b must not land before §6.1b.** The report is explicit: batching alone
  makes first-card latency *worse* — all N cards appear together after generation
  instead of card 1 appearing at block 0's close. *"Landing 2b without 1b is a
  perceived-latency regression."*
- **§6.2d is a security-sensitive commit.** The cache must not let a
  `.biorouterignore` edit go stale — that is a secret-redaction regression, not a
  perf bug. Own commit, own human review. It is worth ~1–3 ms against 1.6 s TTFB,
  so it is hygiene (blocking `std::fs` on a tokio worker on the hottest path),
  not a latency win.
- **§7.10's `isPartial`** is contingent on §6.1b; the `turnActive` half can ship
  independently.

Still-open research question from the report (§9): sample real turns for
tool-argument size distribution. If the p50 tool call has <20 argument tokens,
**§6.1b is a tail fix, not a median fix**, and should be reprioritized below
§6.1a.

---

## 8. Recommended next actions

1. **Rebase onto current `main`** and re-run every gate. Nothing here has been
   tested against the post-dashboard-removal tree.
2. **Separate the boot-splash commits** from the latency track.
3. **Run the versa_azure smoke test** and land the numbers in the register, in
   whichever direction they come out.
4. **Get human review** on `2ef3a56b` (MCP + concurrency, per `HOWTOAI.md`),
   specifically the five-impl safety audit.
5. Decide `docs/design/boot-splash-studio.html`'s home — it violates the
   protected-path rule as filed.

---

## 9. Live smoke-test results (2026-07-18, real UCSF credentials)

Run headless via `biorouter run` against both configured providers, measured
with the Stage-0 markers this branch added. **Supersedes the "unverified"
caveats in §3–§5 for the two Versa providers.**

### 9.1 No regressions

| Check | Result |
|---|---|
| Tool calls execute + correct answers, both providers | PASS (exit 0) |
| Token/cost accounting on the streaming path | PASS — 7 turns, non-zero cost recorded |
| Lock/poison/deadlock errors after the H6 mutex removal | none in server logs |
| GUI renders; trailing indicator + tool card | PASS (verified in a real browser) |

The "4 of 7 extensions loaded" toast is **not** a regression: `ucsfomopagent`
and `cdwagent` want `CLINICAL_RECORDS_USERNAME`, `spokeagent` wants
`SPOKEAGENT_PASSCODE`. Unconfigured secrets, unrelated to this branch.

### 9.2 Streaming: Azure real, Bedrock buffered by the gateway

Measured on an identical ~600-word prose prompt (no tools). A **short**
response cannot distinguish these two cases — both show a small generation
window. Only a long output separates them. Do not re-test this with a short
prompt and conclude anything.

| Provider | `STREAM_OPEN`→`OPENED` (TTFB) | `OPENED`→`EXHAUSTED` (streamed) | Verdict |
|---|---|---|---|
| `versa_azure` / gpt-5.5 | 1.95 s | **10.70 s** | genuine incremental streaming |
| `versa_bedrock` / claude-sonnet-4-6 | 21.93 s | **0.15 s** | fully buffered upstream |

Our Bedrock decoder is lazy (`try_stream!` + `receiver.recv().await` per event,
`formats/bedrock.rs:1130-1145`), so the buffering is **not** ours. The UCSF
MuleSoft gateway (`unified-api.ucsf.edu`) forwards SSE incrementally but
buffers `application/vnd.amazon.eventstream`.

**Decision (owner, 2026-07-18): leave Bedrock streaming enabled as-is.** It is
harmless, and it begins working for free if the gateway is ever fixed. Two
things to know while it stays on:

- It delivers **no** user-visible benefit on Bedrock today.
- Retry semantics changed shape: only stream-open is retried, mid-stream
  failures are terminal. If the gateway starts dropping long streams, turns
  that previously succeeded via retry will now fail. This is the one live
  regression risk being deliberately accepted.

The real fix is a gateway change (pass event-stream through unbuffered), which
would make every Bedrock Claude model stream. Not pursued for now.

### 9.3 The original symptom, measured

```
+0.00s  WAITING_LLM_STREAM_OPEN
+5.39s  WAITING_LLM_STREAM_OPENED    <- the whole perceived gap is here
+5.41s  TOOL_EXEC_START              <- tool fires 20 ms after the response lands
+5.48s  TOOL_EXEC_END                <- tool itself takes 70 ms
+5.56s  WAITING_LLM_STREAM_EXHAUSTED
+5.61s  WAITING_LLM_STREAM_OPEN      <- ~50 ms of bookkeeping before the next turn
```

Tools fire 20 ms after the response arrives and take ~70 ms; inter-turn
bookkeeping is ~50 ms. The 2–5 s gap is model time essentially in full. This
**empirically confirms the investigation's ~90%-intrinsic conclusion** and is
why Track 2 (make the wait legible), not optimization, was the correct fix.

---

## 10. Two bugs found by live GUI testing that the unit tests missed

Both were invisible to the test suites and only surfaced when the app was
actually driven. Recorded because each shows a specific blind spot.

### 10.1 "Maximum update depth exceeded" — PRE-EXISTING, fixed (`d789c6ab`)

Fired on every turn, tearing the transcript down via the ErrorBoundary.
**Confirmed pre-existing**: reproduced identically with the trailing-indicator
commit reverted (1 depth-error either way), so it is not a regression from this
track.

`ProgressiveMessageList`'s batching effect listed `renderedCount` in its own
dependency array while only ever WRITING it through `setRenderedCount(cur => …)`
— it never reads it. It also listed `onRenderingComplete`, whose identity
`BaseChat` rebuilds on every `messages.length` change
(`useCallback(…, [messages.length])`). During a stream the two re-armed each
other: effect → setState → parent re-render → new callback identity → effect.

Fix: hold the callback in a ref, drop both from the dependency array. Verified
live with a `console.error` collector: **1 depth-error before, 0 after.**

**The accompanying test is NOT a gate and says so in-place.** It was checked
against the unfixed component and still passed — jsdom plus Testing Library's
`act()` batching do not reproduce the cascade. The load-bearing evidence is the
live check, not the test.

### 10.2 One transcript bubble per token on Bedrock — REGRESSION, fixed (`0a4b0e57`)

A streamed Bedrock reply rendered a separate message bubble per token, each with
its own timestamp. **This was a real regression from §4's ConverseStream work.**

The desktop store merges streamed messages by `Message::id`
(`chatStreamStore.pushMessage`). The new decoder built every message with
`Message::new(..)` and never set an id, so nothing merged. The Anthropic decoder
gets this for free by reusing the `message_start` id
(`formats/anthropic.rs:629`); ConverseStream's `messageStart` carries only a
role, so the decoder now mints one id per response and stamps every message.

**Why 20 tests missed it:** they all asserted decoded *content* — text, tool
names, arguments, usage — and never once asserted message *identity*. The bug
was structurally outside what they looked at. Two tests added that do gate it,
both verified to FAIL with the id assignment removed
(`every streamed message needs an id: [None, None, None, None]`).

**Generalisable lesson:** a decoder's contract includes the envelope, not just
the payload. Any future streaming decoder needs an "all messages of one response
share one id, and two responses do not" test.

### 10.3 Final GUI verification, both providers

| Check | versa_bedrock (claude-sonnet-4-6) | versa_azure (gpt-5.5) |
|---|---|---|
| Tool call executes, result rendered | PASS | PASS |
| Response is ONE message, one timestamp | PASS (after 10.2) | PASS |
| Trailing activity indicator + elapsed timer | PASS (`Thinking · 23s`) | PASS |
| Tool card status honest while running | PASS | PASS |
| React depth errors | 0 | 0 |
| Cost tracking non-zero | PASS | PASS ($0.04) |

---

## 11. B4 "tab-activation re-submit" — resolved: NOT REPRODUCIBLE, misattributed observation

The §10-era regression sweep for the four deferred items reported a deterministic
live failure (B4): activating a chat tab re-submitted its initial message,
counts 1→2→3→4→5. A dedicated bisect investigation could not reproduce it and
resolved the conflict:

- **Live, isolated environment** (own vite :5273, own user-data-dir, own
  biorouterd): three Home-created tabs, ~15 activations plus a
  pre-session-load race — every session stayed at exactly 1 user message.
- **Instrumented chain**: cargo cleared by `consumePending` at first submit;
  every remount observed `initialMessage=undefined`. No link fails.
- **Code-level bisect**: `git diff 6c261665..HEAD` over the entire
  cargo/auto-submit path (`chatGroups/**`, `ChatGroupsContext.tsx`,
  `navigationUtils.ts`) is EMPTY; `BaseChat.tsx`'s only delta is the additive
  §6.1b pending-card rendering. The guard at HEAD is byte-identical to the
  commit where the same scenario passed live (A4, 6 switches, counts 1+1).
- **Backend excluded**: the re-submit is gated on a frontend-only reducer field
  (`tab.pendingInitialMessage`) that no backend event can write.

Verdict: the B4 "failure" was a confounded observation in a shared environment —
that sweep ran two biorouterds against one `sessions.db` beside a concurrent
session's GUI, and its own B2 runaway (repeated text_editor retries) was
writing the same DB while it clicked. The attribution ("pre-existing gap in
f1f1d6b6") was reasoning-based and is withdrawn.

Kept from the investigation: `activationResubmitGuard.test.tsx` — a
high-fidelity in-process harness (real provider + real urlSync + real reducer,
StrictMode) whose SANITY case proves it is not vacuous: removing the
`consumePending` dispatch makes the same drive re-submit. It guards the
mechanism permanently.

Optional hardening (deliberately NOT done): making the cargo single-shot at the
reducer (e.g. clearing on `openTab` DEDUPE) would remove the dependence on the
component→reducer round-trip — but done naively it can DROP a legitimate first
message when a urlSync echo dedupes before BaseChat consumes. Needs its own
careful change if ever attempted.
