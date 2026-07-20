# Spec 005 — Omics Loom

> **What this is.** The per-app functional and aesthetic rubric verdict for
> `spec-005-omics-loom`, a multi-omics workbench, recording a below-fold transport
> defect and the loss of the first user signal.
> **Status:** Historical record — one successful round plus one provider-blocked round
> from the July 2026 test drive. The run is closed and its defects fed the
> [cumulative findings register](../audit-findings-register.md).
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
- **Reached acceptance** records whether the app cleared that bar. This result set uses `yes`, `no` and `partial`; only the PASS rule above is formally defined in the runbook.
- **`point_clicked`** is one of the app's declared UI signals — the event a click on a volcano-plot point is supposed to deliver to the agent. **STAT3** is the gene the tester selected to fire it.

## Run metadata

| Field | Value |
|---|---|
| App id | `spec-005-omics-loom` |
| Authoring rounds | 1 successful + 1 provider-blocked |
| Reached acceptance | no |
| Channel | CLI (named resumable BioRouter session) |
| Provider/model | `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI) |

## Functional verdict: PARTIAL

| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Four-column omics workbench with interactive volcano, heatmap, network, and inspector |
| Layout matches (5.3) | ⚠️ | All regions exist, but the primary transport starts near y=986 in a 720px viewport |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ✅ | Feature click immediately changed selected feature and inspector; KPI changed 0.74→0.88 after action |
| Agent-driven loop (5.6) | ⚠️ | Workers and actions ran, but repeated describe calls prevented final synthesis/pins and second instruction |
| Multi-agent ran (5.7) | ✅ | Aligner, Correlator, Contrarian, and Weaver all ran on UCSF Azure GPT-5.5 |
| Signals round-trip (5.8) | ⚠️ | Initial `point_clicked` was emitted before subscription |

## Aesthetic verdict: PARTIAL

- The ruled-paper background, taped cards, monochrome charts, compact typography, and dense multi-panel hierarchy strongly match `lab-notebook`.
- Primary transport is below the acceptance viewport and a few heatmap labels clip horizontally.

## Screenshot evidence

> **Note.** The run's `shots/` directory was branch-local to the test worktree and is not
> part of this documentation repository, so the filenames below are references only.

- `spec-005-initial.png`
- `spec-005-integrated.png`

The machine-readable static audit survives at
[`../authoring-logs/spec-005-omics-loom-static.json`](../authoring-logs/spec-005-omics-loom-static.json).

## What worked

- All four workers completed and all sessions were verified on `versa_azure/gpt-5.5-2026-04-24`.
- Successful actions included link brush, feature focus, discordance boxes, concordance KPI 0.879, and eight cross-layer edges.

## Friction encountered

Each item below is rolled up in the
[cumulative findings register](../audit-findings-register.md).

- Direct STAT3 selection updated locally but its signal was not yet subscribed.
- The model repeatedly re-described the unchanged surface between phases; final synthesis and Contrarian pins never rendered before the bounded stop.
- Progress frames were duplicated into both the dedicated status area and inspector/synthesis region.
- The queued refinement hit the UCSF IP-allowlist 403 in 2.7s and made no app change; local retest confirmed the transport and first-signal defects remained.

## Related documentation

- [Cumulative findings register](../audit-findings-register.md) — where the signal-before-subscribe and duplicated-progress-stream findings are written up in full.
- [Agent Drafter 100-app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — defines the 5.2–5.8 rubric checks and the browser-driving procedure.
- [UCSF Azure 403 outage incident](../azure-403-outage-incident.md) — the provider blocker that voided this app's queued refinement.
- [Remediation results](../remediation-results.md) — what was built in response to these findings.
- [Authored-app verdict index](../authored-app-verdict-index.md) — the campaign-wide index of apps, verdicts, and blocked specs.
