# Agent Drafter stress test — hardening fixes

> **What this is.** The defect record for the 100-app Agent Drafter stress test: eight numbered fixes to the drafter, SDK and theme, each with the observed symptom, the root cause, the fix applied, and how it was verified.
> **Status:** Historical record — all shipped fixes in this file landed on 2026-07-11 in commit `679deed5`, "Cherry-pick Agent Drafter H1-H8 fixes from the 100-app stress test". H4 was deliberately **not** shipped; it is recorded below as a deferred candidate. The behaviours described as fixed are the shipped behaviour, so read this for *why* the code looks the way it does.
> **Audience:** developers working on the Agent Drafter, the Apps SDK, or the `ui_*` control tools.
> **Identifier key:** `H<n>` numbers one hardening item from this campaign. This file is the authoritative key — [README.md](README.md) and [per-app-results.md](per-app-results.md) cite `H1`–`H8` and point here for the definitions. App slugs (`smoke-sentiment-lab`, `hyperparam-what-if-lab`, `pk-dose-navigator`, …) are the `id` field of the corresponding spec in [`data/prompts.json`](data/prompts.json), and their per-app outcomes are in [per-app-results.md](per-app-results.md).

These fixes were applied to the drafter, SDK and theme between batches of the stress test. Each one was traced to an observed failure while a real model (GPT-5.5 via `versa_azure`) built and drove apps — none was speculative. Every section below records the symptom, the root cause, the fix, and the verification.

## Summary of the eight items

| Item | Subject | Outcome |
|---|---|---|
| H1 | `ui_panel` could not target an author region | Shipped |
| H2 | Charts were single-series only | Shipped |
| H3 | `ui_state` cascade burned the per-turn action budget | Shipped |
| H4 | No node for styled/raw HTML | **Deferred by design** — not shipped |
| H5 | Stale "No BioRouter backend" banner on a working app | Shipped |
| H6 | Authored text invisible in dark mode | Shipped |
| H7 | Apps fired an agent turn on boot with empty input | Shipped |
| H8 | Iterative "work the list" ask-loops never advanced | Shipped |

---

## H1 — `ui_panel` rejected a `@region:` place

Dashboards-into-regions failed 5–7 times before falling back.

**Symptom** (smoke app `smoke-sentiment-lab`, GPT-5.5). The tool timeline showed `Tool failed appcontrol__ui_panel`. The model called `appcontrol__ui_panel` with:

```json
{"id":"sentiment-dashboard","title":"Sentiment Dashboard","place":"@region:dashboard",
 "body":[{"t":"row","children":[{"t":"stat","label":"Positive","value":2}, …]}]}
```

and the tool rejected it with `" must be one of: dock, left, right, bottom, main, modal"` — **repeated 5–7 times** before the model gave up and used `ui_render` (which loses the panel chrome: title, collapse).

**Root cause.** The single most natural agent action — "mount a titled dashboard card into the author's `@region:dashboard`" — was unsupported. `ui_panel.place` only accepted the SDK-owned dock slots; author regions were reachable only via `ui_render` with a raw `card` node. The model's mental model (panel → a named region) did not match the API, and it burned turns rediscovering that.

**Fix.** `ui_panel.place` was extended to also accept a **target**: `@region:<name>`, `@panel:<id>`, `@main`, or a CSS selector. When `place` is a target, the SDK mounts the panel's card into that element (replacing a same-id panel in place). Dock slots still work unchanged. The tool description and `ui_system_prompt` were updated to say a panel can go into a declared region. Server validation was changed to accept target-form places.

**Verification method.** `cargo test -p biorouter-mcp --lib agent_drafter::control`; re-drive `smoke-sentiment-lab` and confirm 0 `ui_panel` failures with a `@region:` place.

**Result.** Confirmed live on `caravan-route-broker` (app 1): `ui_panel place="@region:ledger"` accepted, 0 tool failures, ledger populated with a panel, 4 stats, a chart and a table.

---

## H2 — charts were single-series only

Multi-series figures (loss curves, comparisons, time-series) failed.

**Symptom** (app 2 `hyperparam-what-if-lab`, GPT-5.5). Moving the learning-rate slider, the agent tried to draw a train-vs-validation loss chart:

```json
{"t":"chart","spec":{"type":"line","title":"Loss curves: AdamW, lr≈3.16e-4 …",
  "series":[{"name":"train", …}, {"name":"val", …}]}}
```

and chart validation rejected it with `-32602: chart spec needs a "data" array`. `ui_panel` failed and **no loss curve rendered** (metrics and advice did). The single most natural ML/finance/science visualization — two lines on one axis — was unrepresentable.

**Root cause.** `ChartSpec`/`renderChart` (`sdk.ts`) and `validate_chart` (`control.rs`) accepted only one series: `{type, title, data:[{label,value}]}`. There was no way to express multiple named series (train/val, actual/forecast, arm-A/arm-B, …).

**Fix.** The chart contract was extended to accept EITHER `data:[{label,value}]` (single, unchanged) OR `series:[{name, data:[{label,value}]}]` (multi). `renderChart` draws one line/bar group per series with a small legend and the palette cycling by series; pie stays single-series (first series or `data`). `validate_chart` accepts `series` (each with a non-empty finite `data`) as an alternative to `data`. Tool descriptions for `ui_chart` and the `chart` node mention `series`.

**Verification method.** Unit tests for `validate_chart` (series form), plus a fallback-stripper rebuild, then re-drive `hyperparam-what-if-lab` and confirm the two-series loss curve renders with a legend and 0 failures.

**Result.** Confirmed: `accepts_multi_series_charts` passes; jsdom render shows 2 polylines and 2 legend swatches; live re-drive of `hyperparam-what-if-lab` rendered `train_loss`/`val_loss` as 2 overlaid lines, and **`toolFails` went from `["ui_panel"]` → `[]`**.

---

## H3 — a `ui_state` cascade burned the turn budget

The turn ended with an awkward "action limit" message.

**Symptom** (app 8 `pk-dose-navigator`, GPT-5.5). The app rendered perfectly (48h vancomycin concentration curve, Cmax/trough/AUC24/%T>MIC tiles, regimen table), but the tool timeline showed **~30+ `appcontrol__ui_state` calls** in a single turn plus repeated `ui_describe`, and the turn ended with:

> "I've reached my action limit for this turn (16 actions without user input), so I'm stopping here rather than because the task is necessarily complete. Would you like me to continue? (raise the cap with max_turns / BIOROUTER_MAX_TURNS)"

The engine's repeated-tool guard also **declined** a surplus `ui_describe`, surfaced as `Tool failed appcontrol__ui_describe`.

**Root cause.** Two compounding things: (1) the model re-called `ui_state` with the same values many times and `ui_describe` repeatedly, each counting against the per-turn action budget; (2) those tools always "succeeded" and emitted a frame even when nothing changed, giving the model no signal to stop, so it looped until the cap.

**Fix.**

- `ui_state`: when `set` is a no-op (every key already equals the stored value) and there is no `remove`, return "no change — you already set these" and **do NOT emit a `state` frame**. Identical repeats become cheap and self-signalling.
- `ui_describe`: append a short "(surface unchanged since your last call)" hint when the reported surface, panels and state are byte-identical to the previous call in this session, so the model does not re-poll.
- `ui_system_prompt`: add an explicit efficiency rule — call `ui_describe` once; do not re-send `ui_state` values you already set; batch UI updates; a turn is not a place to poll.

**Verification method.** Unit tests plus a re-drive of `pk-dose-navigator` after recompile.

**Result.** Unit tests `state_noop_when_unchanged_emits_nothing` and `describe_flags_an_unchanged_repeat` pass (control 32/32). On the re-drive of `pk-dose-navigator` after recompile, the **"reached my action limit" message is GONE** (the turn completes cleanly), the app still renders (curve, 4 tiles, regimen table), and there is no `ui_state` cascade.

> **Residual, not fixed here.** One `ui_describe` still gets declined by the *engine's* repeated-tool guard (in the `biorouter` crate, not the drafter) before the "unchanged" hint can reach the model — cosmetic ("Tool failed" in the timeline; the app is unaffected). The prompt rule and result-hint are the drafter-side levers; hardening the engine's decline-to-timeline labeling is a separate, out-of-scope change.

---

## H4 — no way to render styled or raw HTML

> **Status: deferred candidate — NOT shipped.** H4 is numbered in sequence with the eight items but was assessed and consciously left undone. Do not read it as a delivered fix.

Agents fell back to a text node, which escaped the HTML.

**Symptom** (app 3 `palette-tension-lab`, GPT-5.5). The `@region:preview` "specimen" showed **raw HTML as escaped text** — literal `<div id="specimen-card" style="background:#F3E8D0;…">…</div>` instead of a rendered, themed card. The design agent wants to render an arbitrary styled preview (inline-styled card, button and swatches), but the widget grammar has no HTML/style-bearing node, so it stuffed HTML into a `text` node and the SDK (correctly, for XSS safety) escaped it.

**Assessment.** A real capability gap, but narrower than H1–H3 (mostly creative/design apps) and it collides with the XSS hardening — the app page is NOT a sandboxed iframe. A raw-`html` node would need real sanitization (allow `style` and class, strip `<script>`, `on*=`, `javascript:`), which is meaningful work.

**Interim (cheaper) fix.** Teach agents, via the system prompt and a structured `swatch`/color node, to compose styled previews from existing nodes (card/row/badge/button plus a color `swatch`) rather than raw HTML. Deferred to a later batch's hardening and tracked here so the pattern is on record. For this campaign it was handled per-app via in-session refinement.

---

## H5 — a stale "No BioRouter backend" banner persisted on a working app

**Symptom** (app 20 `argument-cartographer`, GPT-5.5). The app worked perfectly — the agent ran `ui_graph`/`ui_chart`/`ui_render`/`ui_highlight`/`ui_ask`, and the vulnerability table plus the "Strengthen claim N1" ask rendered — yet a red **"No BioRouter backend … Tried: ws://127.0.0.1:3900/apps/app/agent"** banner sat at the top. Diagnosis: the live client was correctly connected (`activeEndpoint` = `…/argument-cartographer/agent`, `wsOpen: true`); the banner was **stale**, left by an earlier failed connect (the default appId `"app"` before config resolved, or a reconnect) and never cleared once a connection succeeded. A working app must never show a backend error.

**Root cause.** `mountBackendError` was only ever added, never removed. `connect()` had no success-side cleanup, so any transient early failure's banner outlived the successful connection.

**Fix.** Added `clearBackendError()` and called it from the `dial()` success path, right after the socket opens. Any stale banner is removed the moment a connection succeeds. This is SDK-only; it reaches served apps via the `sdk_hash` staleness rebuild after the `biorouterd` recompile.

**Verification method and result.** Rebuilt `biorouterd` and reloaded `argument-cartographer`: `bannerPresent: false`, `wsOpen: true`, correct endpoint. The stale banner is gone on a working app.

---

## H6 — authored text was invisible in dark mode

Served apps should default to an explicit (light) theme.

**Symptom** (apps `portfolio-stress-lab` and `incident-triage-war-room`, GPT-5.5). The apps rendered fully and correctly — weights, scenario betas, a Bonds-highlighted loss chart and VaR callouts; a full incident war room — but the masthead intro and region placeholder text were **invisible**. Measured: the agent-browser Chrome is in dark mode (`prefers-color-scheme: dark`), so the design system's dark palette is active (`--br-bg #0d0a06`, `--br-text #fff`), yet the builder authored the intro `<p>` with a **hardcoded dark color** (`rgb(40,34,23)`) that does not adapt, giving dark-on-dark.

**Root cause.** Two things: (1) models author and eyeball apps in light mode and hardcode light-appropriate text colors instead of `var(--br-text*)`; (2) served apps inherit the *viewer's* OS theme via `@media (prefers-color-scheme: dark)`, so a dark-OS viewer silently breaks that hardcoded text. The rendering was non-deterministic with respect to who opens the app.

**Fix (two-pronged).**

- **Deterministic default theme.** The SDK sets `:root[data-br-theme="light"]` on load when the app has not chosen a theme, so an app renders as its author intended (light) regardless of the viewer's OS. The explicit `[data-br-theme]` block wins over the media query. The agent's `ui_theme` still switches it deliberately (for example a dark ops war room), and a manifest opt-in can restore follow-OS.
- **Build guidance and lint.** `create_app`/build instructions mandate `var(--br-text)` / `var(--br-text-muted)` for all text (never hardcode), and `lint_app` warns on a hardcoded `color:` that is not a `var(--br-*)`.

**Verification method and result.** Rebuilt and re-served `portfolio-stress-lab`: `data-br-theme="light"` by default, and the main content is now readable (it was fully dark). The lint was also extended to catch a **surface token used as a text color** (`color:var(--br-muted)`), which is the residual cause of faint secondary text in some already-built apps (invisible in both themes). The guidance and both lints landed in the batch-3 boundary recompile; new builds render readable in either theme.

---

## H7 — apps fired an agent turn on boot with empty input

The empty turn looped and blocked the real turn.

**Symptom** (batch-2 app `model-arena-leaderboard`, GPT-5.5, verified live in the browser twice). With the H6 default-light drafter the app renders beautifully, but driving it exposed a real interaction bug: after clicking the app's own "Demo: ResNet vs ViT" seed button, the roster chips (`resnet50`, `vit-b16`) appear in the Arena DOM, yet the app agent keeps rendering **"the arena is empty / no selected models yet"** into the compare and verdict regions and never draws the radar or table. The Agent-progress timeline showed a long run of `ui_state` calls that never resolved into a dashboard.

**Root cause** (read from the built `src/main.ts`). `runAgent()` was called unconditionally with **no guard on roster size** — on page boot (`runAgent()` at end of file), on `reset`, and on every slider or profile change. So on load the app fired an **empty-roster** turn whose prompt said `Selected models: none`; the agent, given nothing to compare, thrashed `ui_state` and burned its turn on an "empty" render. Because turns in one app session run **one at a time**, that stuck boot turn sat at the head of the queue and the later demo-click turn (which *does* carry the roster) never got to run — so the UI was frozen on the boot turn's "empty" output. This was not a race that a reload fixes; it reproduced on every clean load.

**Fix.**

- *Drafter guidance (systemic; `mod.rs` AGENT-DRIVEN UI gained a new "RUN GUARD" paragraph, plus `build_batch.py` WRAPPER step 3).* Never call `br.run` on boot with an empty form; guard every control handler on the minimum input the task needs (`if (selected.length < 2) return;`); handle empty or partial states locally with a placeholder; and pass the user's current selection **inside** the prompt rather than making the agent `ui_describe` to discover it. This stops the whole class of wasteful and blocking empty turns for every future build (batch 4 onward).
- *In-session app fix (per-app iterative loop).* Resume `ad-model-arena-leaderboard` and have the model add the `< 2 models` guard and drop the boot run, then rebuild — re-verified in the browser.

**Verification method and result.** The CLI and daemon were recompiled with the RUN GUARD guidance at the batch-3 boundary. `model-arena-leaderboard` was fixed in-session (round 2: `< 2 models` guard, boot run removed) and **re-driven live in the browser**: on boot it now shows a local "Add at least 2 models" placeholder and fires **no** agent turn; after the Demo click the agent renders the full dashboard — grouped metric chart, both chips highlighted Pareto-optimal, a leader toast, and a correct verdict (ViT ahead on accuracy and robustness, ResNet on latency, params and memory) — with `ui_state`/`ui_panel`/`ui_chart`/`ui_highlight`×4/`ui_render`/`ui_notify` and 0 failures. The empty-arena freeze is gone.

---

## H8 — iterative "work the list" ask-loops never advanced

They re-asked the same top item.

**Symptom** (batch-3 app `data-quality-triage`, GPT-5.5, verified live). A flagship-quality triage console: masthead, seed-profile stat tiles, a per-column missingness chart, a severity-ranked issue table, and a **blocking `ui_ask`** remediation modal. The ask→apply→ask mechanism worked, but the loop did **not advance**: after clicking "Apply decision" on rank-1 (`is_fraud` 98% one-class, severity 82), the very next `ui_ask` was again about **rank 1** (re-framed for the selected "Billing" risk), and the change log stayed frozen at "Step 0 — baseline health score 58/100". Applied remediations were never marked resolved, so the agent kept re-offering the highest issue instead of descending to rank 2, 3, and so on.

**Root cause.** The app's `system_prompt` drove a per-item loop but never told the agent to (a) persist which items are already resolved or (b) append a per-step log entry. With no memory of "rank 1 is done", the model re-picked the same top issue every turn. This is a whole *class* of app (triage lists, quiz ladders, finding queues), not one app — `mastery-ladder-quiz` only avoided it because each quiz question is naturally new.

**Fix (drafter guidance, systemic).** A new "ITERATIVE LOOPS" paragraph in `mod.rs` (AGENT-DRIVEN UI) plus a line in the `build_batch.py` WRAPPER: for list-working apps the `system_prompt` must track a `resolved`/`done` id set in `ui_state`, always pick the next UNRESOLVED item, append a numbered "Step N — item, choice, score before→after" line to the visible log each turn, and define and check a clear stop condition (render a final summary when done). This prevents the non-advancing loop for every future ask-loop app (batches 5–10).

**Fix (in-session, per-app).** `data-quality-triage` got the same instruction fed back into its build session (resolved-set, per-step log, descend the list), then rebuilt and re-verified. This was queued behind the batch-4 build to avoid concurrent GPT-5.5 sessions contending on the `versa_azure` rate limit.

**Verification method and result.** The guidance was recompiled at the batch-4 boundary. `data-quality-triage` was fixed in-session (round 2: `resolvedIssues` id-set tracked in `ui_state` and passed into every prompt, per-step log append, next-unresolved selection, reset clears it). Re-driven live, the rebuilt app renders the issues table **with a new `status` column**, a clean `Step 0 — baseline … health score 61/100` log line, and a first `ui_ask` correctly scoped to rank 1 (`issue-fraud-imbalance`, severity 82); Apply-decision submits and closes the modal. The full multi-step advance (Step 1 append and descent to rank 2) was left mid-turn because a batch build was saturating the `versa_azure` rate limit — that final confirmation was to be completed in the quiet window between batches. This was not an app defect; the resolved-set machinery is present and firing.

**Re-verification (quiet window).** The H8 machinery was confirmed live on a clean drive: the Step-0 log now reads "…Resolved issue ids: **none**.", the issues table carries a **status** column, and the decision-gate region names the item under decision ("rank 1 — issue-fraud-imbalance … severity 82"). The first `ui_ask` renders correctly scoped to rank 1. The *multi-step* advance capture was foiled by a **hidden-tab WebSocket throttle**, not app logic: the agent-browser tab runs headless and backgrounded, Chrome suspends its WebSocket, and a long multi-minute turn's frames stop delivering mid-turn (the app logs `ws.onclose "connection closed"`; the daemon logs `rmcp: Error sending response`). Real users keep the tab focused, so this does not affect shipped apps.

> **Testing note.** Drive each app in a single pass and capture the first agent render. Do not chase multi-minute multi-turn loops in the headless tab — they get throttled.

## Related documentation

- [Stress test charter and folder index](README.md) — the method, harness and pass criteria these fixes came out of.
- [Per-app results log](per-app-results.md) — the app-by-app record, including the drives that surfaced each `H<n>`.
- [Apps SDK reference](../../apps-sdk/sdk-reference.md) — the current shipped `ui_*` and `br.*` contract, including the multi-series chart and `@region:` panel behaviour added here.
- [Agent Drafter apps platform design](../../agent-drafter/apps-platform-design.md) — how `control.rs`, the SDK and the app socket fit together.
- [App test-drive runbook](../../agent-drafter/testing/app-test-drive-runbook.md) — the browser-driving procedure whose headless-throttle caveat is recorded in H8.
