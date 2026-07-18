# Spec 012 — Contagion Studio

> **What this is.** The per-app functional and aesthetic rubric verdict for
> `spec-012-contagion-studio`, an epidemic-modelling app, and the first record of the
> runtime theme corruption that renders app content as opaque black blocks.
> **Status:** Historical record — a closed July 2026 run. The theme-corruption and
> generic-subagent defects it first surfaced are part of the finished audit corpus; see
> the [cumulative findings register](../audit-findings-register.md) and the
> [remediation results](../remediation-results.md).
> **Audience:** developers working on Agent Drafter and the Apps SDK.

The 100-app test drive asked Agent Drafter to author 100 different scientific apps from
written briefs, then drove each finished app in a real browser to check whether it
behaved as it declared. A *verdict* is the score one app earned against the runbook's
rubric — a functional verdict (does it work as an agent-driven surface?) and an
aesthetic verdict (does it look the way the brief asked?). This file records that
verdict for one app.

## How to read this record

- **`spec-NNN`** identifies a numbered brief in [the 100 agentic app test specs](../../../agent-drafter/testing/hundred-app-test-specs.md); app ids follow `spec-NNN-<slug>`. The campaign-wide roll-up is [the authored-app verdict index](../authored-app-verdict-index.md).
- **Check IDs `5.2`–`5.8`** are rubric sections defined in [the test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) (§5). An app is a functional **PASS** only if 5.2, 5.5, 5.6 and 5.7 all hold and the layout (5.3) substantially matches; §6 scores the aesthetic verdict independently.
- **Reached acceptance** is recorded here as a split verdict — `static yes; runtime partial` — separating the static manifest/source review from the live browser run, as in [spec 011](spec-011-reaction-diffusion-foundry.md). Specs 001–010 use the single-value form.
- **`S/E/I/R`** are the compartments of the SEIR epidemic model the app simulates: susceptible, exposed, infectious, recovered. **`β`** is the transmission rate the user manipulates, and the **attack rate** is the fraction of the population ultimately infected.
- A generic **`subagent`** call delegates to an unnamed helper; a **`consult`** call routes to a *declared worker profile* from the manifest's orchestration block, with its own session and model route. Check 5.7 requires the latter. See the [Apps SDK reference](../../../apps-sdk/sdk-reference.md).

## Run metadata

| Field | Value |
|---|---|
| App id | `spec-012-contagion-studio` |
| Authoring rounds | 1 real round (plus 3 provider-blocked retries, excluded) |
| Reached acceptance | static yes; runtime partial |
| Channel | CLI (named resumable BioRouter session) |
| Provider/model | `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI) |

## Functional verdict: PARTIAL

| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Live S/E/I/R plot, rates rail, KPIs, scenario table, intervention track, and transport dominate. |
| Layout matches (5.3) | ⚠️ | Three-column control-room structure is present, but the lower case-data/composer area and transport labels clip at 1280×720. |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ✅ | β manipulation repainted the curve/KPIs; Add intervention inserted a school-closure marker; Fit to data changed peak and attack-rate values. |
| Agent-driven loop (5.6) | ⚠️ | Fit to data invoked `app_call` and changed outputs, but both tested turns repeated the same orchestration sequence and never completed. |
| Multi-agent ran (5.7) | ❌ | Manifest declares Fitter, Adversary, Policy Analyst, and Reporter, but runtime used two generic `subagent` calls; no declared-profile consults were visible. |
| Signals round-trip (5.8) | ❌ | First intervention gesture reported `marker_dragged` not subscribed. |

## Aesthetic verdict: PARTIAL

- The baseline follows the `clinical` pack with crisp, dense white/steel/coral treatment.
- The live agent theme/render pass then produced large black, illegible blocks over plot/KPI/table content — a severe regression against that baseline, and the reason the verdict is PARTIAL rather than ALIGNED.

> **First sighting.** This app is where runtime theme corruption was first observed. It
> recurs in [spec 013](spec-013-orbital-sandbox.md), [spec 016](spec-016-aerocanvas.md),
> [spec 017](spec-017-automata-loom.md) and [spec 018](spec-018-systemdynamics-forge.md),
> and is registered as `[FUNCTIONAL-BUG][SEV: med] Runtime theme mutation makes
> scientific regions illegible` in the
> [cumulative findings register](../audit-findings-register.md).

## Screenshot evidence

> **Note.** The run's `shots/` directory was branch-local to the test worktree and is not
> part of this documentation repository, so the filenames below are references only.

- `spec-012/baseline.png`
- `spec-012/fit-loop.png`

The machine-readable static audit survives at
[`../authoring-logs/spec-012-contagion-studio-static.json`](../authoring-logs/spec-012-contagion-studio-static.json).

## Friction encountered

Each item below is rolled up in the
[cumulative findings register](../audit-findings-register.md).

- One real Drafter round built cleanly in 275.8 seconds after three outage retries.
- Runtime bypassed all four declared profiles with generic subagents.
- Both Add intervention and Fit to data remained `AI · updating data`; the describe/subscribe/notify/highlight/subagent sequence repeated.
- Runtime theming made major content areas visually unreadable, despite a good baseline.

## Related documentation

- [Cumulative findings register](../audit-findings-register.md) — the full write-up of the theme-corruption and declared-profile-bypass findings, each with a repro.
- [Spec 006 — Ward Board](spec-006-ward-board.md) — where the generic-subagent bypass reproduced here was first isolated.
- [Spec 011 — Reaction-Diffusion Foundry](spec-011-reaction-diffusion-foundry.md) — the preceding simulation app, sharing the split static/runtime acceptance vocabulary.
- [Apps SDK reference](../../../apps-sdk/sdk-reference.md) — defines `consult`, worker profiles, and the `ui_theme` control tool at issue.
- [Remediation results](../remediation-results.md) — what was built in response to these findings.
