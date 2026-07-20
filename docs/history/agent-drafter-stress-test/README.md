# Agent Drafter stress test — 100 sophisticated agentic apps

> **What this is.** The method statement and folder index for a completed campaign that drove a real model to build 100 diverse, non-trivial agentic apps with the Agent Drafter, refining each app in-session and hardening the drafter between batches.
> **Status:** Historical record — the campaign ran to completion. All 100 apps were built and the eight drafter fixes it produced (H1–H8) shipped on 2026-07-11 in commit `679deed5`, "Cherry-pick Agent Drafter H1-H8 fixes from the 100-app stress test". The run described here is over; read this for method and rationale, not as a runbook to execute.
> **Audience:** maintainers of the Agent Drafter, and anyone tracing why an `H<n>` fix exists.
> **Identifier key:** `H<n>` (H1…H8) numbers a drafter/SDK/theme fix made during the campaign; the definitions live in [hardening-fixes.md](hardening-fixes.md). App slugs such as `caravan-route-broker` are the `id` field of the prompt spec in [`data/prompts.json`](data/prompts.json).

The Agent Drafter builds BioRouter apps: a TypeScript front-end wired to a real per-app agent. This campaign existed to push it past one-shot generation into the "new class" of app where the BioRouter backend acts as the intermediary between the LLM and the user across a loop. The pages in this folder are the record of that push — the method (this file), the defects it exposed and fixed, and the per-app outcomes.

## Files in this folder

| File | What it holds |
|---|---|
| `README.md` (this file) | The campaign charter: goal, the two nested iteration loops, engine and sandbox setup, harness scripts, and per-app pass criteria. |
| [hardening-fixes.md](hardening-fixes.md) | The defect record — the eight `H1`–`H8` fixes to the drafter, SDK and theme, each with symptom, root cause, fix and verification. |
| [per-app-results.md](per-app-results.md) | The outcome log — per-app build and pass verdicts, refine-round counts, what each app rendered, per-batch rollups, and the closing marathon summary. |
| [`data/prompts.json`](data/prompts.json) | The 100 app specs that drove the run. A JSON array of 100 objects, each with `id`, `title`, `domain`, `interaction`, a `patterns` array, and a prose `requirement`. |

> **Relationship between the three documents.** This file states the method, `hardening-fixes.md` records what broke in the drafter and how it was fixed, and `per-app-results.md` records what happened app by app. They cover the same run from three angles and cross-reference each other; none supersedes another.

## Goal

Make the Agent Drafter meaningfully more powerful by driving a real model (GPT-5.5 via `versa_azure`) to build **100 diverse, non-trivial agentic apps** — the "new class" where BioRouter's backend is the intermediary between the LLM and the user across a loop, not a one-shot lookup. Each app had to embody at least two of:

- **dashboard** the agent composes from user input (coordinated panels/charts/tables/stats);
- **feedback loop** (`ui_ask`, or user clicks/sliders/drag) where the agent reacts and the loop continues;
- **behavior / pattern tracking** (`ui_state`) the agent adapts to;
- **simulation / what-if** the agent recomputes and re-renders;
- **attention direction** (`ui_highlight` / `ui_layout` / `ui_theme`) as part of the UX.

## Two nested iterative loops

1. **Per-app refinement (within one conversation).** Each app was built in a named session `ad-<id>`. Build → review (static verify, plus driving it in a real browser via agent-browser and judging aesthetics) → feed the concrete problems back into the SAME session (`build_batch.py fix <id> "<issues>"`) → the model fixes and rebuilds → repeat until the app is genuinely good (works and looks right). Rounds were tracked in `build-logs/rounds.json`.

2. **Per-batch drafter hardening (every 5–10 apps).** Recurring problems that were the *drafter's* fault (SDK, `ui_*` tools, theme, export, prompting defaults) were fixed in the worktree, tests added, binaries recompiled, and the next batch run against the improved drafter. Logged in [hardening-fixes.md](hardening-fixes.md).

## Engine and sandbox

> **Note.** The values below were specific to this run. The port, the model pin and the sandbox layout are configuration choices of the campaign, not fixed properties of the Agent Drafter. The absolute filesystem location of the worktree and of the sandbox `XDG_CONFIG_HOME` was not recorded in this document.

- Build and app model: **GPT-5.5** (`versa_azure/gpt-5.5-2026-04-24`), keyless via `BIOROUTER_DISABLE_KEYRING=true` plus the plaintext `secrets.yaml` copied into a sandbox `XDG_CONFIG_HOME`. Apps lived in `.ad-sandbox/.config/biorouter/agent_drafter/`.
- Binaries: the **worktree** `target/debug/{biorouter,biorouterd}` (carrying every hardening fix). Rebuilt after each `H<n>`.
- Serving daemon: `biorouterd agent` on port 3900, with sandboxed `HOME` and `XDG_CONFIG_HOME`.

## Harness

- `scripts/ad-stress/build_batch.py build <prompts.json> <start> <count>` — builds a batch, auto-retrying a failed build once in-session.
- `scripts/ad-stress/build_batch.py fix <id> "<issues>"` — the per-app refine round, in the same chat.
- `scripts/ad-stress/verify.mjs <base> --all` — the static gate (app serves, bundle carries the UI runtime, manifest is agentic and ui-enabled, prompt directs `ui_*` tools, regions declared, socket advertises `ui`).
- Browser and aesthetics: agent-browser opened each built app, drove its loop and took screenshots; craft was judged by the reviewer and problems went back through `fix`.

## Pass criteria (per app)

Static gate green **and**, in the browser: the declared controls drive the agent; the agent composes the intended dashboard/loop with `ui_*` tools (panels/charts/regions populate, 0 unexpected `Tool failed` frames); a feedback interaction actually loops; and the result reads as a crafted UI (coherent layout, hierarchy, spacing, on-brand), not a wireframe — after however many refine rounds it took.

## Related documentation

- [Hardening fixes (H1–H8)](hardening-fixes.md) — the eight drafter defects this campaign found and fixed, with root causes.
- [Per-app results log](per-app-results.md) — what each of the 100 apps actually did, batch by batch.
- [Agent Drafter apps platform design](../../agent-drafter/apps-platform-design.md) — the design of the system being stress-tested.
- [App test-drive runbook](../../agent-drafter/testing/app-test-drive-runbook.md) — the current procedure for driving a built app in a browser.
- [Apps SDK reference](../../apps-sdk/sdk-reference.md) — the `br.*` and `ui_*` surface whose gaps H1–H8 closed.
