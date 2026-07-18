# Layout-diversity audit and controlled probes

## Finding

The recurring left rail + center stage + right inspector + bottom transport
shape is primarily imposed by the 100-idea corpus, not yet evidence that Agent
Drafter is incapable of other layouts.

A mechanical audit of every `**Layout:**` line in
`docs/agentic-app-test-ideas-100.md` found:

| Corpus property | Count |
|---|---:|
| Explicitly mentions `Left` | 100 / 100 |
| Explicitly mentions `Center` | 100 / 100 |
| Explicitly mentions `Right` | 100 / 100 |
| Explicitly mentions `Bottom` | 100 / 100 |
| Uses the word `rail` | 89 / 100 |
| Uses the word `inspector` | 96 / 100 |

The first 14 generated apps therefore reflect their locked design orders. They
do show different themes and central widgets, but they do not constitute a fair
structural-diversity test.

## What Agent Drafter claims to support

The shipped implementation exposes five structured non-chat starters plus chat:

| Archetype | Starter shape |
|---|---|
| `explorer` | network/graph + search + detail |
| `dashboard` | KPI mosaic + narrative/detail |
| `workbench` | dense table + selected-row detail |
| `wizard` | centered staged form |
| `canvas` | author-registered full draw surface |
| `chat` | legacy chat card |

`docs/agent-drafter-apps.md` explicitly says apps should look and interact
differently. The starters are not identical: dashboard and wizard do not seed
the same two-column arrangement used by explorer/workbench/canvas. However,
three of the five structured starters do begin from a two-column card grid, so
starter gravity is a secondary risk even though the idea corpus is the dominant
cause of the observed three-panel repetition.

## Corrective protocol for Specs 016–100

The locked regions and dimensions remain mandatory. Starting with Spec 016, the
Agent Drafter order also requires concept-specific spatial hierarchy, responsive
behavior, controls, and visual rhythm, and explicitly prohibits copying a prior
app's generic rail/card/inspector CSS. Per-app aesthetic rubrics will record both
theme alignment and whether the composition is meaningfully distinct.

This does not rewrite the 100 ideas: every requested region remains present.
Where a spec itself mandates three columns, diversity must come from the
composition inside and between those regions rather than silently violating the
spec.

## Controlled Agent Drafter probes

The 100 prompts cannot prove structural range because all 100 prescribe the
same four-way skeleton. Five additional, clearly labeled probe apps will be
authored through Agent Drafter and retained in the worktree-local BioRouter app
store. They are evidence probes, not substitutes for any numbered application.

| Probe id | Required archetype | Layout constraint |
|---|---|---|
| `layout-probe-kpi-mosaic` | `dashboard` | asymmetric KPI mosaic with top command ribbon and bottom narrative drawer; no persistent sidebars |
| `layout-probe-centered-wizard` | `wizard` | single centered stepper with progressive disclosure and full-width completion canvas; no sidebars |
| `layout-probe-radial-canvas` | `canvas` | full-bleed radial canvas with floating tool petals and a modal sheet; no left/right columns |
| `layout-probe-tabletop-workbench` | `workbench` | full-width table under a top filter ribbon, with an expandable bottom detail drawer |
| `layout-probe-constellation` | `explorer` | full-bleed network, command palette overlay, and transient popover dossier rather than a persistent inspector |

Each probe must:

- be authored only through `create_app` / `update_app` / `configure_app` /
  `build_app` in a named Agent Drafter session;
- use only `versa_azure/gpt-5.5-2026-04-24` for main/worker/routes;
- remain under `.br-testdrive/runtime/config/biorouter/agent_drafter/`;
- build/lint cleanly, include responsive media rules, and pass desktop browser
  checks (plus narrow visual checks when the browser surface supports resizing);
- demonstrate a direct-manipulation control and one agent-driven action; and
- have screenshot and per-probe evidence recorded beside the 100-app results.

## Results

All five probe apps were authored through named Agent Drafter sessions, remain
in the isolated BioRouter store, and use the UCSF model. Static evidence is in
`layout-probes/static-audit.json`; per-probe browser rubrics and screenshots are
under `layout-probes/results/` and `layout-probes/shots/`.

| Probe | Static | Structural diversity | Functional | Aesthetic |
|---|---|---|---|---|
| KPI mosaic | pass | pass | partial | aligned |
| Centered wizard | pass | pass | partial | aligned |
| Radial canvas | pass after theme refinement | pass | partial | aligned after refinement |
| Tabletop workbench | pass after theme refinement | pass | partial | partial at runtime |
| Constellation explorer | pass after theme refinement | pass | partial | aligned baseline / partial runtime |

The controlled result is decisive: Agent Drafter can generate materially
different configurations and looks when the prompt asks for them. The observed
numbered-app homogeneity is therefore primarily a test-corpus constraint, with
starter gravity as a secondary risk rather than a hard layout limitation.

The probes also show that geometry diversity does not fix the runtime control
bugs: all five lost the first `probe_adjusted` signal, all five entered a
repeated control-plane call pattern, and several exposed blank/stale bindings.
Those defects remain tracked independently in `FINDINGS.md`.

## Status

Corpus audit, five-probe authoring, static audit (5/5), and desktop browser
verification are complete. The in-app browser did not expose viewport resizing,
so narrow behavior is evidenced by authored media rules rather than a narrow
screenshot. Future numbered-app prompting includes the anti-template guard.
