# Spec 001 — Variant Tribunal

> **What this is.** The per-app functional and aesthetic rubric verdict for
> `spec-001-variant-tribunal`, one app authored and browser-verified during the Agent
> Drafter Apps SDK v2 100-app test drive.
> **Status:** Historical record — a completed browser-verified authoring run from the
> July 2026 test drive. Its findings were rolled up into the
> [cumulative findings register](../audit-findings-register.md) and acted on in the
> [remediation results](../remediation-results.md), so this file is frozen evidence
> rather than living reference.
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
- **Finding tags** of the form `[TYPE][severity]` — `ERGONOMICS`, `AUTHORING-INEFFICIENCY`, `SPEC-GAP`, `FUNCTIONAL-BUG`, `SECURITY/ROBUSTNESS` — index into the [cumulative findings register](../audit-findings-register.md), where each entry carries a repro, root cause, impact, and suggested fix.
- **Reached acceptance** records whether the app cleared that bar. This result set uses `yes`, `no` and `partial`; only the PASS rule above is formally defined in the runbook.

## Run metadata

| Field | Value |
|---|---|
| App id | `spec-001-variant-tribunal` |
| Authoring rounds | 6 |
| Reached acceptance | partial |
| Channel | CLI authoring + in-app browser verification |
| Archetype chosen by the agent | canvas |
| Provider/model | `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI), verified in authoring, main runtime, and worker session rows |

## Functional verdict: PARTIAL

| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Genome evidence workbench is primary; only a 340px footer note composer is secondary. |
| Layout matches (5.3) | ✅ | 260px rail, 678px center, 340px verdict inspector, 64px transport, floating presence; eight named regions. |
| Declared surface (5.4) | ✅ | All 6 actions, 4 signals, 3 custom components, state schema, and 4 profiles declared and wired. |
| Client reactivity (5.5) | ✅ | Clicking PVS1 immediately lit the criterion, added it to the verdict card, and changed the presence text before the model completed. |
| Agent-driven loop (5.6) | ⚠️ | One full loop reached `ui_describe → consult×4 → app_call×multiple → ui_notify`, patched evidence tracks, and changed VUS confidence 42%→62%. A later second instruction entered a runaway repeated-`ui_describe` failure, so repeatability failed. |
| Multi-agent ran (5.7) | ✅ | Separate UCSF sessions and attributed consults ran for `prosecutor`, `defense`, `clerk`, and `chief_justice`. |
| Signals round-trip (5.8) | ❌ | First gesture surfaced `signal "criterion_clicked" is not subscribed`; explicit `ui_subscribe` was added and completed on the next turn, but that turn ran away before a post-subscription gesture could be verified. |

## Aesthetic verdict: ALIGNED

- `clinical` pack applied in light mode; coherent warm clinical palette and system typography.
- Dense but readable three-column courtroom layout, correct transport placement, restrained black/amber/green/red semantics, and an ambient presence chip.
- No page overflow at 1280×720. Named-region geometry matches the spec closely.

## Screenshot evidence

> **Note.** The run's `shots/` directory was branch-local to the test worktree and is not
> part of this documentation repository, so the filenames below are references only.

- `spec-001-pre-runtime-fix.png` — complete authored workbench before runtime refinement.
- `spec-001-agent-loop.png` was attempted after the completed loop, but CDP screenshot capture timed out twice; final dynamic state is preserved in the DOM/session trace and authoring logs.

The machine-readable static audit for this app survives at
[`../authoring-logs/spec-001-variant-tribunal-static.json`](../authoring-logs/spec-001-variant-tribunal-static.json).

## Friction encountered

Each tag below indexes an entry in the
[cumulative findings register](../audit-findings-register.md).

- `[ERGONOMICS][high]` Agent Drafter store bypassed `BIOROUTER_PATH_ROOT`; the incomplete draft was quarantined and XDG isolation added.
- `[AUTHORING-INEFFICIENCY][med]` six rejected nested manifest/orchestration shapes before convergence.
- `[SPEC-GAP][high]` invented invalid KB id `br.kb`.
- `[FUNCTIONAL-BUG][high]` profile display-name/key mismatch plus UI-enabled worker stalled the first loop.
- `[FUNCTIONAL-BUG][high]` signal was not subscribed until a later refinement.
- `[SECURITY/ROBUSTNESS][high]` main agent repeatedly retried `ui_describe` after the tool result explicitly said the user declined and ordered it not to retry; this made the second instruction runaway.

## Related documentation

- [Agent Drafter 100-app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — defines the 5.2–5.8 rubric checks and the per-app result format used here.
- [Cumulative findings register](../audit-findings-register.md) — the de-duplicated roll-up where each finding tag above is written up in full.
- [Remediation results](../remediation-results.md) — what was actually built and shipped in response to these findings.
- [Spec 002 — Cohort Funnel Foundry](spec-002-cohort-funnel-foundry.md) — the next app in the run, which reproduced this file's repeated-`ui_describe` engine defect.
- [Authored-app verdict index](../authored-app-verdict-index.md) — the campaign-wide index of apps, verdicts, and blocked specs.
