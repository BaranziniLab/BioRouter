# Agent Drafter remediation — what was built, and what it caught

**Branch:** `feat/apps-sdk-v2` · **Commits:** `ae8987a6`, `7527f848`, `d8cf95cc`
**Plan:** [REMEDIATION-PLAN.md](REMEDIATION-PLAN.md) · **Audit:** [FINDINGS.md](FINDINGS.md)

---

## 1. The thesis, and whether it held

The audit's finding was not "Agent Drafter is bad at building apps". It builds the
static shell perfectly — not-a-chatbot 18/18, prescribed layout 11/11, declared
surface 25/25. The finding was:

> **Everything the platform *checks*, the model gets right. Everything the platform
> merely *asks for in a system prompt*, the model gets wrong** — signals 1/12,
> agent-driven loop 0/3, zero full functional passes.

So the remediation was never "write better instructions". Every fix moves one clause
of the contract out of the system prompt and into one of four enforcement points:

| Enforcement | Meaning |
|---|---|
| **A schema the model is handed** | It cannot emit a shape the schema forbids. |
| **A tool absent from the tool list** | It cannot call what it cannot see. |
| **A server check that fails closed** | An invalid manifest cannot be saved or built. |
| **A check that executes** | A control that delivers no turn fails the build. |

That thesis held under implementation. Repeatedly, the code turned out to be *worse*
than the audit reported — and worse in the same direction: the platform was not
failing to prevent the model's mistakes, it was **causing** them.

---

## 2. The four times the platform was the culprit

These are the findings where reading the source changed the diagnosis.

### The runaway tool loop was BioRouter's own guard

The audit said the engine "neither masks the declined tool nor terminates repeated
calls". True — but the decline the model saw came from `RepetitionInspector`, and:

- it tracked only the **immediately preceding** call, so an alternating `A,B,A,B`
  reset the counter every iteration and **never tripped at all**;
- the deny answered *"The user has declined to run this tool"* — **a lie**, and one
  the model could not see through, so it could not learn to do something different;
- and the loop **simply continued**. Nothing removed the tool from the next provider
  request, so the model retried to the turn cap — every iteration a billed call.

The whole guard was, literally, a sentence in a tool result.

### Workers were *instructed* to seize the UI

The audit read this as model drift. It is a deny-by-default inversion.
`UiCapability::enabled` defaults `true` — correct for the main agent, whose blast
radius is its own page. But a worker profile is a full `AgentConfig`, so a profile
authored *without* a `ui` block deserialized as `true`, and `validate_profiles` ANDed
`true && true`. Every worker was handed `appcontrol` on the **main bridge** plus the
`ui_system_prompt` whose first rule is *"drive the page"*. Prose telling them not to
was competing with the tools they had just been given.

### Both delegation mechanisms were armed at once

`orchestration.sub_agents` registered recipes for the generic `subagent` tool while
`orchestration.agents` armed `consult`. The generic one is easier to reach — its
description auto-lists the very worker names the author registered, and it takes a
free-form `instructions` string. `spec-006-ward-board` declared the same four workers
**twice**, once in each map, and the declared profiles were dead configuration.

### "Both workers timed out and main silently completed" was guaranteed

Two racing timers on either side of a channel. The `consult` tool started a 120 s
timer *before* the request reached the socket loop; `run_consult` started a second one
strictly later. The outer always won, so the inner was dead code — and when the outer
fired, the loop was still awaiting the worker, **draining nothing**. When the abandoned
worker finally answered, `resolve_consult` found no pending entry and **threw the
answer away**. Paid work, discarded. And the deadline was a compile-time constant with
**no configuration path at all**.

---

## 3. Two findings the audit never saw

Both surfaced while verifying the plan's citations.

**A literal NUL byte in `sdk.ts` (line 4446).** `grep` classifies the file as binary
and prints nothing; `git diff` shows *"Binary files differ"*. The most-reviewed file in
the feature was **unsearchable and unreviewable** — and the review pass that found this
had already, briefly, dismissed a whole cluster of real findings as fabricated because
of it.

**The client half of the signals bug.** `emitSignal` fire-and-forgets through a
`send()` that returns `false` when the socket is not `OPEN`, with no queue. A signal
fired during page load never left the browser *at all*. Both ends were dropping the
same gesture, for different reasons — so the server-side fix alone would not have
worked.

---

## 4. What was built

| Wave | Change | Enforcement |
|---|---|---|
| **0** | NUL byte removed + CI guard; one env-aware path resolver; resolved `read_app` view; `TurnAborted` + real exit codes (75 auth / 70 provider / 76 loop / 77 worker) | check |
| **1** | `list_platform_catalog`; write-boundary rejection of unknown KB/skill/extension ids; `requires{}`; `capability_report` frame; never arm a tool for an unsatisfiable grant | schema + check |
| **2** | `declare_surface` / `declare_profiles` / `set_theme` / `set_routes`; merge-on-write manifests; surface seeded from round 0 | schema |
| **3.1** | **Declaration IS subscription** — `SignalDecl.eager` + a client outbox | check |
| **3.2** | One canonical state doc: `state_initial`, `data-br-model` two-way binding, canonical doc attached to every typed turn with disagreements named | check |
| **3.3** | `effect: mutate` + `writes` → handler-owned pointers the agent cannot write; `app_call` readback reports what actually moved | check |
| **3.4/3.5** | `worker_ui` (deny-by-default); tolerant-but-unambiguous profile-key resolution; generated orchestration prompt; `subagent` **withheld from the tool list** when profiles exist | absent tool |
| **3.6** | `report_evidence` (workers only — the main agent cannot write its own alibi); per-turn evidence ledger; `requires_evidence` + `provenance_required` enforced at `app_call`; synthetic values badged | check |
| **4.1** | Turn guard: signature-keyed repetition across the turn; blocked tool **removed from the provider request**; second attempt terminates; honest loop-blocked message | absent tool |
| **4.2** | One deadline owner; expiry **cancels** the worker; configurable per profile; timeout is an `is_error` result, not prose; `done{degraded, missingProfiles}` + banner | check |
| **4.3** | `run()` resolves its target synchronously, paints "Queued…", and a watchdog drains a stalled chain | check |
| **4.4** | Progress-sink registry — tool frames stop displacing the science | check |
| **4.5** | `ui_theme` audits contrast and **reverts**, reporting a `ui_error`; a `--br-plot-*` token layer so authored SVG is theme-reactive | check |
| **5.2** | `br.dnd.catalog` — pointer + click + keyboard parity, and it **emits the declared signal itself** | primitive |
| **5** | Lint: drag-only surface is an Error naming `br.dnd.catalog`; bindings with no `state_initial` warn and explain why the obvious workaround *is* the bug | executing-adjacent |

---

## 5. What it catches on the **real** corpus

Not on fixtures — on the 30 apps Agent Drafter actually produced during the audit,
re-linted by the fixed platform (`tests/testdrive_corpus_relint.rs`):

| Check | Apps caught (of 30) |
|---|---|
| Invented knowledge-base / skill ids — now **rejected at the write boundary** | **19** |
| `data-br-bind` with no `state_initial` (renders blank until a *paid* turn) | **30** |
| Hand-rolled HTML5 drag (unreachable by keyboard, touch, or any automated pointer) | **10** |
| **Still load under the new schema** (back-compat) | **30 / 30** |

That last row is the one that matters for the ~110 v1 apps in the wild. Waves 1–5 added
nine manifest fields — `requires`, `state_initial`, `effect`, `writes`,
`requires_evidence`, `provenance_required`, `eager`, `worker_ui`, `consult_timeout_s` —
and every one defaults. Had any been made required, all 30 of these would have failed
to deserialize.

The 19 rejections are not abstract. They are `phenotype-defs`, `clinical-guidelines`,
`hpo-omim-gene-disease`, `ecology-parameter-ranges`, `statistical-genetics` — ids the
model invented because the manifest had **no vocabulary for "I need this and it isn't
here"**. It does now.

---

## 6. Tests

| Suite | Result |
|---|---|
| `biorouter-mcp` | 719 pass |
| `biorouter-server` | 101 pass (+2 network-only tunnel tests, unrelated) |
| `biorouter` | 750 pass (+1 known-flaky `gcpauth` timing race, passes 12/12 in isolation) |
| `biorouter-cli` | 182 pass |

New suites, each pinned to a specific audit finding: `template_text_integrity`,
`path_resolver_agreement`, `turn_abort_tests`, `catalog_write_boundary`,
`typed_declaration`, `eager_signals`, `action_effects`, `evidence_gate`, `turn_guard`,
`lint_interaction`, `testdrive_corpus_relint`.

Three pre-existing tests **asserted the old, broken contracts** and were rewritten —
which is itself evidence the bugs were baked in, not accidental:

- `ui_is_shared_when_both_grant_it` asserted that a default-config worker *gets the
  UI*. That assertion was the inversion.
- `validate_signal_checks_subscription…` asserted that a declared signal is refused
  until the agent subscribes. That assertion was the 1/12 bug.
- The repetition tests asserted consecutive-only counting — the reason an interleaved
  loop never tripped.

`fallback_strips_real_sdk_template_into_valid_js` earned its keep: it caught that the
no-esbuild fallback bundler cannot strip tuple types or function-type return
annotations, so the new contrast-audit and `br.dnd` code would have shipped as **broken
JS on any machine without esbuild**.

---

## 7. What is *not* done

Honest ledger:

- **The executing smoke runner (`app-smoke.mjs`) is not built.** Its lint-rule half
  landed (drag-only, blank bindings), but the Chromium tier that clicks every control
  and asserts a frame reaches the wire is still the plan's Wave 5.1. That is the check
  that would catch a *dead control* — the one class of defect no static rule can see.
  The SDK-side causes of dead controls (the wedged run queue, the pre-paint throw) are
  fixed; the executing check that would *prove* it per-app is not.
- **The 30 corpus apps are not re-authored.** They were built against the broken
  platform; the fixed platform now *sees* their defects, but making them pass means
  re-running the authoring loop, not patching them.
- **The `done{degraded}` banner and contrast audit are unit- and build-tested, not
  browser-tested.** Both are client-side behaviours whose real proof is a browser run.
