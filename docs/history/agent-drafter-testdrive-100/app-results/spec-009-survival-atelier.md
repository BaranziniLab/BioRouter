# Spec 009 — Survival Atelier

> **What this is.** The per-app functional and aesthetic rubric verdict for
> `spec-009-survival-atelier`, a survival-analysis studio — one of only two outright
> functional FAILs in the run, caused by a drag-only interaction with no accessible
> fallback.
> **Status:** Historical record — a closed July 2026 run (one successful round, one
> provider-blocked). The accessibility defect it isolated is part of the finished audit
> that drove the [remediation results](../remediation-results.md).
> **Audience:** developers working on Agent Drafter and the Apps SDK.

The 100-app test drive asked Agent Drafter to author 100 different scientific apps from
written briefs, then drove each finished app in a real browser to check whether it
behaved as it declared. A *verdict* is the score one app earned against the runbook's
rubric — a functional verdict (does it work as an agent-driven surface?) and an
aesthetic verdict (does it look the way the brief asked?). This file records that
verdict for one app.

## How to read this record

- **`spec-NNN`** identifies a numbered brief in [the 100 agentic app test specs](../../../agent-drafter/testing/hundred-app-test-specs.md); app ids follow `spec-NNN-<slug>`. The campaign-wide roll-up is [the authored-app verdict index](../authored-app-verdict-index.md).
- **Check IDs `5.2`–`5.8`** are rubric sections defined in [the test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) (§5). An app is a functional **PASS** only if 5.2, 5.5, 5.6 and 5.7 all hold and the layout (5.3) substantially matches.
- **CUA** is the computer-use browser-automation harness used to drive the finished app (Playwright MCP or equivalent, per §4.2 of the runbook). "A CUA drag" means a real pointer drag issued by that harness, not a synthetic DOM event.
- **KM** means Kaplan–Meier survival curve; a **stratum** is a covariate grouping the user builds before a Cox model can be fitted.

> **Why a functional FAIL can still be aesthetically ALIGNED.** The runbook scores the
> two verdicts independently — §5 asks whether the app works, §6 asks whether it looks
> the way the brief specified. This app was built beautifully and was simply unusable:
> the only path to its core interaction was an HTML5 drag that did not respond.

## Run metadata

| Field | Value |
|---|---|
| App id | `spec-009-survival-atelier` |
| Authoring rounds | 1 successful + 1 provider-blocked |
| Reached acceptance | no |
| Channel | CLI (named resumable BioRouter session) |
| Provider/model | `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI) |

## Functional verdict: FAIL

| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Survival-analysis studio with covariate rail, KM canvas/risk table, forest dock, inspector, and transport |
| Layout matches (5.3) | ✅ | Required rail/canvas/inspector/fixed transport are present; below-fold rail/table content scrolls |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ❌ | CUA drag could not add a stratum; slider reset; failed gestures blanked initialized bindings |
| Agent-driven loop (5.6) | ❌ | Core stratum prerequisite was unreachable, so no valid turn or second instruction could run |
| Multi-agent ran (5.7) | ❌ | No valid stratum turn; no worker profile executed |
| Signals round-trip (5.8) | ❌ | No stratum/cutpoint signal was successfully emitted |

## Aesthetic verdict: ALIGNED

- Elegant serif `journal` treatment, large KM canvas, muted geometry, floating C-index, forest dock, and fixed transport match the brief.
- Body scroll height is 1008px, but the primary transport remains visible at 720p and the composition is coherent.

## Screenshot evidence

> **Note.** The run's `shots/` directory was branch-local to the test worktree and is not
> part of this documentation repository, so the filename below is a reference only.

- `spec-009-initial.png`

The machine-readable static audit survives at
[`../authoring-logs/spec-009-survival-atelier-static.json`](../authoring-logs/spec-009-survival-atelier-static.json).

## Friction encountered

Each item below is rolled up in the
[cumulative findings register](../audit-findings-register.md), whose entry for this
defect carries the reproduction steps this file does not: at 1280x720, drag either
`[draggable=true]` covariate card into the stratum builder, then click **Fit Cox**.

- HTML5 drag was the only stratum-creation path and did not respond to two real CUA drags; no accessible fallback existed.
- Failed gestures caused initialized binding values to disappear; keyboard slider changes reset to 65.
- Guarded **Fit Cox** correctly requested a stratum but misleadingly entered an AI-updating status with no session message.
- Manifest declares unavailable/unverified `clinical-biostatistics` skill.
- The queued refinement hit the UCSF IP-allowlist 403 in 5.7s and made no app change; local retest confirmed covariates remained drag-only and inaccessible.

## Related documentation

- [Cumulative findings register](../audit-findings-register.md) — the full write-up of the drag-only-with-no-fallback defect, with its repro and the proposed lint for drag/click parity.
- [Agent Drafter 100-app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — defines the 5.2–5.8 rubric checks and the browser-driving procedure.
- [Spec 002 — Cohort Funnel Foundry](spec-002-cohort-funnel-foundry.md) — where the same HTML5 drag limitation was first observed, and where Agent Drafter did add a click fallback.
- [Spec 004 — Trial Regia](spec-004-trial-regia.md) — the run's other clinical-statistics app, which shares the `clinical-biostatistics` skill problem.
- [Remediation results](../remediation-results.md) — what was built in response to these findings.
