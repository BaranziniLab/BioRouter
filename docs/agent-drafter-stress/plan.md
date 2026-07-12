# Agent Drafter stress test — 100 sophisticated agentic apps

Goal: make the Agent Drafter meaningfully more powerful by driving a real model
(GPT-5.5 via versa_azure) to build **100 diverse, non-trivial agentic apps** — the
"new class" where Biorouter's backend is the intermediary between the LLM and the
user across a loop, not a one-shot lookup. Each app must embody ≥2 of:

- **dashboard** the agent composes from user input (coordinated panels/charts/tables/stats);
- **feedback loop** (ui_ask, or user clicks/sliders/drag) where the agent reacts and the loop continues;
- **behavior / pattern tracking** (ui_state) the agent adapts to;
- **simulation / what-if** the agent recomputes and re-renders;
- **attention direction** (ui_highlight / ui_layout / ui_theme) as part of the UX.

## Two nested iterative loops

1. **Per-app refinement (within one conversation).** Each app is built in a named
   session `ad-<id>`. Build → I review (static verify + drive it in a real browser
   via agent-browser + judge aesthetics) → I feed the concrete problems back into
   the SAME session (`build_batch.py fix <id> "<issues>"`) → it fixes and rebuilds
   → repeat until the app is genuinely good (works + looks right). Rounds are
   tracked in `build-logs/rounds.json`.

2. **Per-batch drafter hardening (every 5–10 apps).** Recurring problems that are
   the *drafter's* fault (SDK, `ui_*` tools, theme, export, prompting defaults) get
   fixed in the worktree, tests added, binaries recompiled, and the next batch runs
   against the improved drafter. Logged in [`hardening.md`](hardening.md).

## Engine & sandbox

- Build + app model: **GPT-5.5** (`versa_azure/gpt-5.5-2026-04-24`), keyless via
  `BIOROUTER_DISABLE_KEYRING=true` + the plaintext `secrets.yaml` copied into a
  sandbox `XDG_CONFIG_HOME`. Apps live in `.ad-sandbox/.config/biorouter/agent_drafter/`.
- Binaries: the **worktree** `target/debug/{biorouter,biorouterd}` (carry every
  hardening fix). Rebuild after each `H<n>`.
- Serving daemon: `biorouterd agent` on port 3900, sandboxed HOME/XDG.

## Harness

- `scripts/ad-stress/build_batch.py build <prompts.json> <start> <count>` — builds a batch,
  auto-retries a failed build once in-session.
- `scripts/ad-stress/build_batch.py fix <id> "<issues>"` — the per-app refine round (same chat).
- `scripts/ad-stress/verify.mjs <base> --all` — static gate (serves, bundle has the UI
  runtime, manifest agentic + ui, prompt directs ui_* tools, regions declared, socket
  advertises `ui`).
- Browser + aesthetics: agent-browser opens each built app, drives its loop, screenshots,
  and I judge craft; problems go back through `fix`.

## Pass criteria (per app)

Static gate green **and** in the browser: the declared controls drive the agent; the
agent composes the intended dashboard/loop with ui_* tools (panels/charts/regions
populate, 0 unexpected `Tool failed` frames); a feedback interaction actually loops;
and the result reads as a crafted UI (coherent layout, hierarchy, spacing, on-brand),
not a wireframe — after however many refine rounds it takes.

## Log

Per-app outcomes, rounds, and process notes: [`results.md`](results.md).
Drafter fixes: [`hardening.md`](hardening.md). Prompts: [`prompts.json`](prompts.json).
